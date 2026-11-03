//! C ABI facade over [`show_player_core`] for the WinUI shell.
//!
//! Contract (ABI version 3; versions 1 and 2 were the C++ core's):
//!
//! - Commands and queries are protobuf `Request` messages passed to `ab_call`;
//!   the reply is a `Response` held in a Rust-owned result handle. Bytes from
//!   `ab_result_bytes` stay valid until `ab_result_free`.
//! - Status codes report transport outcomes only. Domain failures are
//!   `Response.error`.
//! - `ab_create` sets `*out_core` to null on failure and, when it can, sets
//!   `*out_error` to a result holding a `Response` with an `Error`. Every
//!   other output is null on failure.
//! - A core handle may be used from any thread, including concurrently.
//!   Playback commands queue work and return at once. `ListBooks` reads the
//!   folder and the store on the calling thread, so the shell calls it off its
//!   UI thread.
//! - Events are polled: `ab_wait_events` blocks for up to `timeout_ms` until
//!   at least one event is queued and returns an `EventBatch`. After
//!   `ab_shutdown` has succeeded and every event has been taken it returns
//!   `AB_SHUT_DOWN`. Position events are coalesced while the shell is behind.
//! - `ab_shutdown` saves the position, stops the engine and closes the store;
//!   no event is queued after it succeeds. On `AB_TIMEOUT` the position is
//!   already saved and the process may exit.
//! - `ab_destroy` must not overlap any other call on the handle, including a
//!   waiting `ab_wait_events`. It shuts down first if that has not happened.
//! - After a contained panic the handle is unusable: `ab_call` and
//!   `ab_wait_events` return `AB_UNUSABLE`; `ab_shutdown` and `ab_destroy`
//!   remain valid.
//! - Memory is freed only by the side that allocated it.

pub mod convert;

#[allow(clippy::all)]
pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/jupiter.show_player.v1.rs"));
}

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use prost::Message;
use show_player_core::library;
use show_player_core::player::{
    DEFAULT_SAVE_INTERVAL, ErrorCode, EventSink, LogLevel, Player, PlayerConfig, PlayerEvent,
};
use show_player_core::store::Reader;

use messages::{ErrorKind, EventType, request, response};

pub const ABI_VERSION: u32 = 3;

pub const AB_OK: i32 = 0;
pub const AB_INVALID_ARGUMENT: i32 = 1;
pub const AB_MALFORMED_REQUEST: i32 = 2;
pub const AB_PANIC: i32 = 3;
pub const AB_UNUSABLE: i32 = 4;
pub const AB_CREATE_FAILED: i32 = 5;
pub const AB_TIMEOUT: i32 = 6;
pub const AB_SHUT_DOWN: i32 = 7;

/// Queued events beyond this are dropped, oldest first.
const EVENT_LIMIT: usize = 4096;
const DEFAULT_RECENT: usize = 10;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
struct Pending {
    events: VecDeque<messages::Event>,
    dropped: u32,
    closed: bool,
}

#[derive(Default)]
pub struct EventQueue {
    pending: Mutex<Pending>,
    signal: Condvar,
}

impl EventQueue {
    fn push(&self, event: messages::Event) {
        let mut pending = lock(&self.pending);
        // A position superseded before the shell has read it is replaced.
        let coalesce = event.r#type == EventType::Position as i32
            && pending
                .events
                .back()
                .is_some_and(|last| last.r#type == EventType::Position as i32 && last.generation == event.generation);
        if coalesce {
            *pending.events.back_mut().expect("checked above") = event;
        } else {
            if pending.events.len() >= EVENT_LIMIT {
                pending.events.pop_front();
                pending.dropped = pending.dropped.saturating_add(1);
            }
            pending.events.push_back(event);
        }
        drop(pending);
        self.signal.notify_all();
    }

    fn close(&self) {
        lock(&self.pending).closed = true;
        self.signal.notify_all();
    }

    /// Waits up to `timeout` for events. `None` means closed and drained.
    fn take(&self, timeout: Duration) -> Option<messages::EventBatch> {
        let pending = lock(&self.pending);
        let mut pending = self
            .signal
            .wait_timeout_while(pending, timeout, |p| p.events.is_empty() && !p.closed)
            .map(|(guard, _)| guard)
            .unwrap_or_else(|e| e.into_inner().0);
        if pending.events.is_empty() && pending.closed {
            return None;
        }
        let events = pending.events.drain(..).collect();
        let dropped = std::mem::take(&mut pending.dropped);
        Some(messages::EventBatch { events, dropped })
    }
}

struct QueueSink(Arc<EventQueue>);

impl EventSink for QueueSink {
    fn event(&mut self, event: &PlayerEvent) {
        self.0.push(convert::event_to_message(event));
    }

    fn log(&mut self, level: LogLevel, message: &str) {
        self.0.push(messages::Event {
            r#type: EventType::Log as i32,
            log_level: level as i32,
            message: message.to_owned(),
            ..Default::default()
        });
    }
}

pub struct CoreHandle {
    data_dir: PathBuf,
    shows: show_player_core::shows::Service,
    player: Player,
    events: Arc<EventQueue>,
    reader: Mutex<Reader>,
    poisoned: AtomicBool,
}

pub struct ResultHandle {
    bytes: Vec<u8>,
}

fn result(message: &impl Message) -> *mut ResultHandle {
    Box::into_raw(Box::new(ResultHandle { bytes: message.encode_to_vec() }))
}

fn error_response(request_id: u64, kind: ErrorKind, message: impl Into<String>) -> messages::Response {
    messages::Response {
        request_id,
        result: Some(response::Result::Error(messages::Error { kind: kind as i32, message: message.into() })),
    }
}

fn player_error(request_id: u64, code: ErrorCode) -> messages::Response {
    let message = match code {
        ErrorCode::InvalidArgument => "The request has an invalid value.",
        ErrorCode::ShuttingDown => "The player is shutting down.",
        _ => "The player refused the request.",
    };
    error_response(request_id, convert::error_kind(code), message)
}

/// Reads `len` bytes at `ptr`; a null pointer is valid only with length zero.
///
/// # Safety
/// `ptr` must be valid for reads of `len` bytes for the duration of the call.
unsafe fn input<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees `ptr` is readable for `len` bytes during this call.
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// The BerthaML show database shipped beside the executable.
fn show_database() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("data/berthaml.db"))).unwrap_or_default()
}

/// Dispatches one decoded request.
pub fn dispatch(core: &CoreHandle, req: messages::Request) -> messages::Response {
    let id = req.request_id;
    let ok = |r: response::Result| messages::Response { request_id: id, result: Some(r) };
    let finish = |r: Result<(), ErrorCode>| match r {
        Ok(()) => ok(response::Result::Done(messages::Empty {})),
        Err(code) => player_error(id, code),
    };
    let books = |r: Result<Vec<library::BookEntry>, String>, ranked: bool| match r {
        Ok(mut list) => {
            let preferences = show_player_core::catalogue::Priorities::load(&core.data_dir);
            if ranked {
                list.sort_by_cached_key(|b| {
                    let mut p = show_player_core::catalogue::classify(&b.evidence, &b.path, "", "", "", "");
                    p.kind = b.source_kind.clone();
                    p.modifiers = b.modifiers.clone();
                    (show_player_core::catalogue::rank(&p, &preferences), b.title.clone())
                });
            }
            ok(response::Result::Books(convert::books_to_message(&list)))
        }
        Err(e) => error_response(id, ErrorKind::Io, e),
    };
    let player = &core.player;
    use request::Command as C;
    match req.command {
        Some(C::InspectShow(s)) => match show_player_core::shows::inspect(Path::new(&s.folder), &s.notes_path) {
            Ok(mut d) => {
                match show_player_core::catalogue::reference(&show_database(), &d.date, &d.venue) {
                    Ok(Some(reference)) => {
                        d.reference_info = reference.info;
                        if let Some(place) = reference.place {
                            d.set_place(&place);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => d.reference_info = e,
                }
                if let Ok(link) =
                    show_player_core::linking::link(&show_database(), &show_player_core::linking::import_evidence(&d))
                {
                    d.set_link(&link);
                }
                ok(response::Result::ShowDraft(d.into()))
            }
            Err(e) => error_response(id, ErrorKind::Io, e),
        },
        Some(C::SearchRecordings(s)) => {
            match show_player_core::catalogue::search(
                &show_database(),
                &s.query,
                &s.source,
                &show_player_core::catalogue::Priorities::load(&core.data_dir),
            ) {
                Ok(rows) => ok(response::Result::Recordings(messages::RecordingList {
                    items: rows.into_iter().map(Into::into).collect(),
                })),
                Err(e) => error_response(id, ErrorKind::Io, e),
            }
        }
        Some(C::GetPriorities(_)) => {
            ok(response::Result::Priorities(show_player_core::catalogue::Priorities::load(&core.data_dir).into()))
        }
        Some(C::SetPriorities(p)) => match show_player_core::catalogue::Priorities::from(p).save(&core.data_dir) {
            Ok(p) => ok(response::Result::Priorities(p.into())),
            Err(e) => error_response(id, ErrorKind::InvalidArgument, e),
        },
        Some(C::PackageShow(s)) => match s.draft {
            Some(d) => match core.shows.package(d.into(), PathBuf::from(s.output), s.copy_audio) {
                Ok(j) => ok(response::Result::Job(j.into())),
                Err(e) => error_response(id, ErrorKind::InvalidArgument, e),
            },
            None => error_response(id, ErrorKind::InvalidArgument, "No show draft was provided."),
        },
        Some(C::TagShow(s)) => match s.draft {
            Some(d) => match core.shows.tag_files(d.into(), PathBuf::from(s.folder)) {
                Ok(j) => ok(response::Result::Job(j.into())),
                Err(e) => error_response(id, ErrorKind::InvalidArgument, e),
            },
            None => error_response(id, ErrorKind::InvalidArgument, "No show draft was provided."),
        },
        Some(C::GetJob(_)) => ok(response::Result::Job(core.shows.snapshot().into())),
        Some(C::CancelJob(_)) => {
            core.shows.cancel();
            ok(response::Result::Done(messages::Empty {}))
        }
        Some(C::Open(o)) => finish(player.open(&o.path, id)),
        Some(C::CloseBook(_)) => finish(player.close_book()),
        Some(C::Play(_)) => finish(player.play()),
        Some(C::Pause(_)) => finish(player.pause()),
        Some(C::SeekAbsolute(s)) => finish(player.seek_absolute(s.seconds)),
        Some(C::SeekRelative(s)) => finish(player.seek_relative(s.seconds)),
        Some(C::ChapterSelect(c)) => finish(player.select_chapter(c.index)),
        Some(C::ChapterNext(_)) => finish(player.next_chapter()),
        Some(C::ChapterPrevious(_)) => finish(player.previous_chapter()),
        Some(C::SetVolume(v)) => finish(player.set_volume(v.volume)),
        Some(C::SetMute(m)) => finish(player.set_mute(m.mute)),
        Some(C::GetSnapshot(g)) => {
            ok(response::Result::Snapshot(convert::snapshot_to_message(&player.snapshot(), g.include_chapters)))
        }
        Some(C::ListBooks(l)) => {
            if l.folder.is_empty() {
                return error_response(id, ErrorKind::InvalidArgument, "No folder was given.");
            }
            books(library::list_folder(&mut lock(&core.reader), Path::new(&l.folder), &show_database()), true)
        }
        Some(C::RecentBooks(r)) => {
            let limit = if r.limit == 0 { DEFAULT_RECENT } else { r.limit as usize };
            books(library::recent(&mut lock(&core.reader), limit, &show_database()), false)
        }
        None => error_response(id, ErrorKind::UnknownCommand, "Unknown or missing command."),
    }
}

/// Creates a core from a `CoreConfig` with no FFI involved; used by `ab_create` and tests.
pub fn create(config: messages::CoreConfig) -> Result<CoreHandle, Box<messages::Response>> {
    let fail = |kind: ErrorKind, message: String| Box::new(error_response(0, kind, message));
    let data_dir = if config.data_dir.is_empty() {
        show_player_core::default_data_dir()
            .ok_or_else(|| fail(ErrorKind::InvalidArgument, "LOCALAPPDATA is not set.".into()))?
    } else {
        PathBuf::from(&config.data_dir)
    };
    let engine_path = if config.engine_path.is_empty() {
        show_player_core::default_engine_path()
            .ok_or_else(|| fail(ErrorKind::InvalidArgument, "The program folder is unknown.".into()))?
    } else {
        PathBuf::from(&config.engine_path)
    };
    let save_interval = if config.save_interval_ms == 0 {
        DEFAULT_SAVE_INTERVAL
    } else {
        Duration::from_millis(u64::from(config.save_interval_ms))
    };
    let events = Arc::new(EventQueue::default());
    let player = Player::start(
        PlayerConfig {
            data_dir: data_dir.clone(),
            engine_path,
            audio_output: (!config.audio_output.is_empty()).then_some(config.audio_output),
            save_interval,
        },
        Box::new(QueueSink(Arc::clone(&events))),
    )
    .map_err(|e| fail(convert::error_kind(e.code), e.message))?;
    let shows = show_player_core::shows::Service::default();
    shows.record_sources_in(data_dir.clone());
    Ok(CoreHandle {
        data_dir: data_dir.clone(),
        shows,
        player,
        events,
        reader: Mutex::new(Reader::new(&data_dir)),
        poisoned: AtomicBool::new(false),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ab_abi_version() -> u32 {
    ABI_VERSION
}

/// # Safety
/// `config_ptr` must be readable for `config_len` bytes; `out_core` and
/// `out_error` must be writable pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ab_create(
    config_ptr: *const u8,
    config_len: usize,
    out_core: *mut *mut CoreHandle,
    out_error: *mut *mut ResultHandle,
) -> i32 {
    if out_core.is_null() {
        return AB_INVALID_ARGUMENT;
    }
    // SAFETY: checked non-null; the caller guarantees it is writable.
    unsafe { *out_core = std::ptr::null_mut() };
    if !out_error.is_null() {
        // SAFETY: the caller guarantees it is writable when non-null.
        unsafe { *out_error = std::ptr::null_mut() };
    }
    // SAFETY: forwarded caller guarantee.
    let Some(bytes) = (unsafe { input(config_ptr, config_len) }) else { return AB_INVALID_ARGUMENT };
    let outcome = catch_unwind(|| {
        let config = messages::CoreConfig::decode(bytes).map_err(|_| None)?;
        create(config).map_err(Some)
    });
    match outcome {
        Ok(Ok(core)) => {
            // SAFETY: checked non-null above.
            unsafe { *out_core = Box::into_raw(Box::new(core)) };
            AB_OK
        }
        Ok(Err(None)) => AB_MALFORMED_REQUEST,
        Ok(Err(Some(error))) => {
            if !out_error.is_null() {
                // SAFETY: checked non-null.
                unsafe { *out_error = result(error.as_ref()) };
            }
            AB_CREATE_FAILED
        }
        Err(_) => AB_PANIC,
    }
}

/// # Safety
/// `core` must be a live handle from `ab_create`; `request_ptr` readable for
/// `request_len` bytes; `out_result` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ab_call(
    core: *const CoreHandle,
    request_ptr: *const u8,
    request_len: usize,
    out_result: *mut *mut ResultHandle,
) -> i32 {
    if core.is_null() || out_result.is_null() {
        return AB_INVALID_ARGUMENT;
    }
    // SAFETY: checked non-null; the caller guarantees it is writable.
    unsafe { *out_result = std::ptr::null_mut() };
    // SAFETY: the caller guarantees a live handle.
    let core = unsafe { &*core };
    if core.poisoned.load(Ordering::Acquire) {
        return AB_UNUSABLE;
    }
    // SAFETY: forwarded caller guarantee.
    let Some(bytes) = (unsafe { input(request_ptr, request_len) }) else { return AB_INVALID_ARGUMENT };
    let Ok(request) = messages::Request::decode(bytes) else { return AB_MALFORMED_REQUEST };
    match catch_unwind(AssertUnwindSafe(|| dispatch(core, request))) {
        Ok(response) => {
            // SAFETY: checked non-null above.
            unsafe { *out_result = result(&response) };
            AB_OK
        }
        Err(_) => {
            core.poisoned.store(true, Ordering::Release);
            AB_PANIC
        }
    }
}

/// # Safety
/// `core` must be a live handle; `out_result` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ab_wait_events(
    core: *const CoreHandle,
    timeout_ms: u32,
    out_result: *mut *mut ResultHandle,
) -> i32 {
    if core.is_null() || out_result.is_null() {
        return AB_INVALID_ARGUMENT;
    }
    // SAFETY: checked non-null; the caller guarantees it is writable.
    unsafe { *out_result = std::ptr::null_mut() };
    // SAFETY: the caller guarantees a live handle.
    let core = unsafe { &*core };
    if core.poisoned.load(Ordering::Acquire) {
        return AB_UNUSABLE;
    }
    let timeout = Duration::from_millis(u64::from(timeout_ms.min(60_000)));
    match catch_unwind(AssertUnwindSafe(|| core.events.take(timeout))) {
        Ok(Some(batch)) => {
            // SAFETY: checked non-null above.
            unsafe { *out_result = result(&batch) };
            AB_OK
        }
        Ok(None) => AB_SHUT_DOWN,
        Err(_) => {
            core.poisoned.store(true, Ordering::Release);
            AB_PANIC
        }
    }
}

/// `timeout_ms` < 0 waits without limit.
///
/// # Safety
/// `core` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ab_shutdown(core: *const CoreHandle, timeout_ms: i32) -> i32 {
    if core.is_null() {
        return AB_INVALID_ARGUMENT;
    }
    // SAFETY: the caller guarantees a live handle.
    let core = unsafe { &*core };
    core.shows.shutdown();
    let timeout = u64::try_from(timeout_ms).ok().map(Duration::from_millis);
    match catch_unwind(AssertUnwindSafe(|| core.player.shutdown(timeout))) {
        Ok(Ok(())) => {
            core.events.close();
            AB_OK
        }
        Ok(Err(ErrorCode::Timeout)) => AB_TIMEOUT,
        Ok(Err(_)) => AB_INVALID_ARGUMENT,
        Err(_) => AB_PANIC,
    }
}

/// # Safety
/// `result` must be a live result handle; `out_ptr` and `out_len` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ab_result_bytes(
    result: *const ResultHandle,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> i32 {
    if result.is_null() || out_ptr.is_null() || out_len.is_null() {
        return AB_INVALID_ARGUMENT;
    }
    // SAFETY: the caller guarantees a live result and writable outputs.
    unsafe {
        let bytes = &(*result).bytes;
        *out_ptr = bytes.as_ptr();
        *out_len = bytes.len();
    }
    AB_OK
}

/// # Safety
/// `result` must be null or a result handle not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ab_result_free(result: *mut ResultHandle) {
    if !result.is_null() {
        // SAFETY: created by Box::into_raw in `result` and freed once.
        drop(unsafe { Box::from_raw(result) });
    }
}

/// # Safety
/// `core` must be null or a handle not yet destroyed, with no other call on it in progress.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ab_destroy(core: *mut CoreHandle) {
    if core.is_null() {
        return;
    }
    // SAFETY: created by Box::into_raw in `ab_create` and destroyed once. The
    // player's drop shuts it down if needed; a thread that does not stop is
    // left detached with everything it uses.
    let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(core) })));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(core: *const CoreHandle, request: messages::Request) -> messages::Response {
        let bytes = request.encode_to_vec();
        let mut out = std::ptr::null_mut();
        // SAFETY: a live handle and a local buffer.
        assert_eq!(unsafe { ab_call(core, bytes.as_ptr(), bytes.len(), &mut out) }, AB_OK);
        let (mut ptr, mut len) = (std::ptr::null(), 0usize);
        // SAFETY: a live result.
        assert_eq!(unsafe { ab_result_bytes(out, &mut ptr, &mut len) }, AB_OK);
        // SAFETY: valid until freed below.
        let response = messages::Response::decode(unsafe { std::slice::from_raw_parts(ptr, len) }).unwrap();
        unsafe { ab_result_free(out) };
        response
    }

    #[test]
    fn create_reports_a_missing_engine_as_a_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let config = messages::CoreConfig {
            data_dir: dir.path().to_str().unwrap().into(),
            engine_path: dir.path().join("absent.dll").to_str().unwrap().into(),
            ..Default::default()
        }
        .encode_to_vec();
        let (mut core, mut error) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: local buffers and outputs.
        let status = unsafe { ab_create(config.as_ptr(), config.len(), &mut core, &mut error) };
        assert_eq!(status, AB_CREATE_FAILED);
        assert!(core.is_null() && !error.is_null());
        let (mut ptr, mut len) = (std::ptr::null(), 0usize);
        unsafe { ab_result_bytes(error, &mut ptr, &mut len) };
        let response = messages::Response::decode(unsafe { std::slice::from_raw_parts(ptr, len) }).unwrap();
        unsafe { ab_result_free(error) };
        match response.result {
            Some(response::Result::Error(e)) => assert_eq!(e.kind, ErrorKind::Engine as i32),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn malformed_input_is_a_status() {
        let garbage = [0xFFu8, 0xFF, 0xFF];
        let mut core = std::ptr::null_mut();
        // SAFETY: local buffers.
        assert_eq!(unsafe { ab_create(garbage.as_ptr(), 3, &mut core, std::ptr::null_mut()) }, AB_MALFORMED_REQUEST);
        assert_eq!(unsafe { ab_create(std::ptr::null(), 3, &mut core, std::ptr::null_mut()) }, AB_INVALID_ARGUMENT);
        assert_eq!(
            unsafe { ab_call(std::ptr::null(), garbage.as_ptr(), 3, &mut std::ptr::null_mut()) },
            AB_INVALID_ARGUMENT
        );
        assert_eq!(ab_abi_version(), ABI_VERSION);
    }

    /// Runs only where libmpv is present (set AB_TEST_ENGINE to its path).
    #[test]
    fn round_trip_through_the_abi() {
        let Some(engine) = std::env::var_os("AB_TEST_ENGINE") else { return };
        let dir = tempfile::tempdir().unwrap();
        let config = messages::CoreConfig {
            data_dir: dir.path().to_str().unwrap().into(),
            engine_path: engine.to_str().unwrap().into(),
            audio_output: "null".into(),
            ..Default::default()
        }
        .encode_to_vec();
        let mut core = std::ptr::null_mut();
        assert_eq!(unsafe { ab_create(config.as_ptr(), config.len(), &mut core, std::ptr::null_mut()) }, AB_OK);

        let snapshot = call(
            core,
            messages::Request {
                request_id: 1,
                command: Some(request::Command::GetSnapshot(messages::GetSnapshot { include_chapters: true })),
            },
        );
        assert!(matches!(snapshot.result, Some(response::Result::Snapshot(_))));
        assert_eq!(snapshot.request_id, 1);
        let missing = dir.path().join("absent.mka").to_str().unwrap().to_owned();
        let opened = call(
            core,
            messages::Request {
                request_id: 2,
                command: Some(request::Command::Open(messages::Open { path: missing })),
            },
        );
        assert!(matches!(opened.result, Some(response::Result::Done(_))));
        let unknown = call(core, messages::Request { request_id: 3, command: None });
        assert!(matches!(unknown.result, Some(response::Result::Error(_))));

        // The missing book is reported by an event answering request 2.
        let mut answered = false;
        for _ in 0..50 {
            let mut out = std::ptr::null_mut();
            assert_eq!(unsafe { ab_wait_events(core, 100, &mut out) }, AB_OK);
            let (mut ptr, mut len) = (std::ptr::null(), 0usize);
            unsafe { ab_result_bytes(out, &mut ptr, &mut len) };
            let batch = messages::EventBatch::decode(unsafe { std::slice::from_raw_parts(ptr, len) }).unwrap();
            unsafe { ab_result_free(out) };
            if batch.events.iter().any(|e| e.r#type == EventType::LoadFailed as i32 && e.request_id == 2) {
                answered = true;
                break;
            }
        }
        assert!(answered);
        assert_eq!(unsafe { ab_shutdown(core, 3000) }, AB_OK);
        let mut out = std::ptr::null_mut();
        while unsafe { ab_wait_events(core, 0, &mut out) } == AB_OK {
            unsafe { ab_result_free(out) };
        }
        assert_eq!(unsafe { ab_wait_events(core, 0, &mut out) }, AB_SHUT_DOWN);
        unsafe { ab_destroy(core) };
    }
}
