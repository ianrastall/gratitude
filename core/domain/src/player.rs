//! The player. One thread owns the engine, the state machine and the store;
//! everything else reaches it through the command queue and reads state
//! through immutable snapshots.
//!
//! Positions are logical: after a seek the position is the target until the
//! engine has arrived there, so a snapshot or a save never shows the old
//! place or a value a moment short of the target. Saving is guarded so that a
//! position is written only for the book it belongs to, once something has
//! moved it, and never past the end.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{JoinHandle, ThreadId};
use std::time::{Duration, Instant};

use crate::mpv::{END_FILE_REASON_ERROR, Engine, Event, Node, Value};
use crate::store::Store;
use crate::text::{dedupe_title, trim_chapter_title};

/// Playback states, numbered as in every earlier ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum State {
    Idle = 0,
    Loading = 1,
    Paused = 2,
    Playing = 3,
    Finished = 4,
    Missing = 5,
    Failed = 6,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Loading => "loading",
            State::Paused => "paused",
            State::Playing => "playing",
            State::Finished => "finished",
            State::Missing => "missing",
            State::Failed => "failed",
        }
    }
}

/// Why a book is missing or failed, or a command was refused. The values are
/// those of the earlier `ab_status`, which the test scripts print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum ErrorCode {
    #[default]
    None = 0,
    InvalidArgument = -1,
    InvalidState = -2,
    NotFound = -3,
    Io = -4,
    Store = -5,
    Engine = -6,
    Unsupported = -7,
    ShuttingDown = -8,
    Timeout = -9,
    Internal = -10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    StateChanged,
    BookLoaded,
    LoadFailed,
    Position,
    ChapterChanged,
    Finished,
    SaveFailed,
    SaveRecovered,
    VolumeChanged,
}

impl EventKind {
    pub fn name(self) -> &'static str {
        match self {
            EventKind::StateChanged => "state",
            EventKind::BookLoaded => "book_loaded",
            EventKind::LoadFailed => "load_failed",
            EventKind::Position => "position",
            EventKind::ChapterChanged => "chapter",
            EventKind::Finished => "finished",
            EventKind::SaveFailed => "save_failed",
            EventKind::SaveRecovered => "save_recovered",
            EventKind::VolumeChanged => "volume",
        }
    }
}

/// The player's state at the moment an event was raised.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerEvent {
    pub kind: EventKind,
    /// Incremented by every open and close.
    pub generation: u64,
    /// The open request this answers (book loaded, load failed); otherwise 0.
    pub request_id: u64,
    pub state: State,
    pub error: ErrorCode,
    /// mpv error code, 0 if none.
    pub engine_error: i32,
    /// Current chapter index, -1 if none.
    pub chapter: i32,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub mute: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error = 0,
    Warning = 1,
    Info = 2,
}

/// Receives events and log messages on the player thread, one call at a
/// time. It must return quickly and must not call [`Player::shutdown`].
pub trait EventSink: Send + 'static {
    fn event(&mut self, event: &PlayerEvent);
    fn log(&mut self, level: LogLevel, message: &str);
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chapter {
    pub title: String,
    pub start: f64,
    pub end: f64,
}

/// Immutable once published; shared between successive snapshots of a book.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookInfo {
    pub path: String,
    pub title: String,
    pub author: String,
    pub comment: String,
    pub codec: String,
    pub file_size: i64,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub state: State,
    pub generation: u64,
    pub error: ErrorCode,
    pub engine_error: i32,
    pub chapter: i32,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub mute: bool,
    /// The position is not being saved.
    pub save_failed: bool,
    /// The stored record was finished when the book was opened.
    pub was_finished: bool,
    /// The book loaded most recently, in this or an earlier run; empty if none.
    pub last_book: String,
    pub book: Option<Arc<BookInfo>>,
}

pub struct PlayerConfig {
    pub data_dir: PathBuf,
    /// libmpv-2.dll, by absolute path.
    pub engine_path: PathBuf,
    /// The engine's audio output driver; `None` selects its default and "null" is silent.
    pub audio_output: Option<String>,
    /// Time between saves while playing.
    pub save_interval: Duration,
}

pub const DEFAULT_SAVE_INTERVAL: Duration = Duration::from_millis(5000);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Command {
    Open { path: String, request_id: u64 },
    Close,
    Play,
    Pause,
    SeekAbsolute(f64),
    SeekRelative(f64),
    ChapterSelect(i32),
    ChapterNext,
    ChapterPrevious,
    Volume(f64),
    Mute(bool),
    Shutdown,
}

struct Queue {
    commands: VecDeque<Command>,
    stopping: bool,
    engine_alive: bool,
}

struct Shared {
    engine: Engine,
    queue: Mutex<Queue>,
    snapshot: Mutex<Arc<Snapshot>>,
    done: Mutex<bool>,
    done_signal: Condvar,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic on the player thread is contained there; the data behind these
    // locks is always left consistent, so a poisoned lock is still usable.
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// The playback core. Every method may be called from any thread. Methods
/// that change playback queue a request and return at once; the outcome
/// arrives as an event.
pub struct Player {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    thread_id: ThreadId,
    joined: AtomicBool,
}

// Reply ids of the observed properties.
const PROP_TIME: u64 = 1;
const PROP_PAUSE: u64 = 2;
const PROP_EOF: u64 = 3;
const PROP_DURATION: u64 = 4;
const PROP_VOLUME: u64 = 5;
const PROP_MUTE: u64 = 6;

/// Seconds from the end that still count as the end.
const END_TOLERANCE: f64 = 2.0;
/// Previous-chapter restarts the chapter past this many seconds into it.
const PREVIOUS_THRESHOLD: f64 = 5.0;
/// Playback this far past a seek target releases it.
const TARGET_RELEASE: f64 = 0.5;
const SEEK_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const POSITION_EVENT_INTERVAL: Duration = Duration::from_millis(250);
const VOLUME_SAVE_DEBOUNCE: Duration = Duration::from_millis(1000);
const SEEK_SETTLE_TIMEOUT: Duration = Duration::from_secs(3);

impl Player {
    /// Opens the store, loads and starts the engine and starts the player thread.
    pub fn start(config: PlayerConfig, mut sink: Box<dyn EventSink>) -> Result<Player, StartError> {
        let fail = |sink: &mut Box<dyn EventSink>, code, message: String| {
            sink.log(LogLevel::Error, &message);
            StartError { code, message }
        };
        let (store, notice) =
            Store::open(&config.data_dir).map_err(|e| fail(&mut sink, ErrorCode::Store, format!("store: {e}")))?;
        if let Some(notice) = notice {
            sink.log(LogLevel::Warning, &notice);
        }
        let engine = Engine::create(&config.engine_path).map_err(|e| fail(&mut sink, ErrorCode::Engine, e))?;

        let volume =
            store.get_setting("volume").and_then(|v| v.parse::<f64>().ok()).map_or(100.0, |v| v.clamp(0.0, 100.0));
        let mute = store.get_setting("mute").is_some_and(|m| m == "1");
        let last_book = store.get_setting("last_book").unwrap_or_default();

        // keep-open is mandatory: without it a start position at the end
        // fails the load, and reaching the end unloads the file.
        let volume_text = format!("{volume:.1}");
        let mut options = vec![
            ("vid", "no"),
            ("audio-display", "no"),
            ("terminal", "no"),
            ("config", "no"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("ytdl", "no"),
            ("load-scripts", "no"),
            ("volume", volume_text.as_str()),
            ("mute", if mute { "yes" } else { "no" }),
            ("audio-client-name", "Show Player"),
        ];
        if let Some(ao) = config.audio_output.as_deref() {
            options.push(("ao", ao));
        }
        for (name, value) in options {
            if let Err(code) = engine.set_option(name, value) {
                sink.log(LogLevel::Warning, &format!("option {name}: {}", engine.error_string(code)));
            }
        }
        if let Err(code) = engine.initialize() {
            let message = format!("mpv_initialize: {}", engine.error_string(code));
            return Err(fail(&mut sink, ErrorCode::Engine, message));
        }
        engine.observe_double(PROP_TIME, "time-pos");
        engine.observe_flag(PROP_PAUSE, "pause");
        engine.observe_flag(PROP_EOF, "eof-reached");
        engine.observe_double(PROP_DURATION, "duration");
        engine.observe_double(PROP_VOLUME, "volume");
        engine.observe_flag(PROP_MUTE, "mute");

        let initial = Snapshot {
            state: State::Idle,
            generation: 0,
            error: ErrorCode::None,
            engine_error: 0,
            chapter: -1,
            position: 0.0,
            duration: 0.0,
            volume,
            mute,
            save_failed: false,
            was_finished: false,
            last_book: last_book.clone(),
            book: None,
        };
        let shared = Arc::new(Shared {
            engine,
            queue: Mutex::new(Queue { commands: VecDeque::new(), stopping: false, engine_alive: true }),
            snapshot: Mutex::new(Arc::new(initial)),
            done: Mutex::new(false),
            done_signal: Condvar::new(),
        });
        let worker = Worker::new(Arc::clone(&shared), store, sink, config.save_interval, volume, mute, last_book);
        let thread =
            std::thread::Builder::new().name("show-player player".into()).spawn(move || worker.run()).map_err(|e| {
                StartError { code: ErrorCode::Internal, message: format!("cannot start the player thread: {e}") }
            })?;
        let thread_id = thread.thread().id();
        Ok(Player { shared, thread: Mutex::new(Some(thread)), thread_id, joined: AtomicBool::new(false) })
    }

    fn post(&self, command: Command) -> Result<(), ErrorCode> {
        let mut queue = lock(&self.shared.queue);
        if queue.stopping {
            return Err(ErrorCode::ShuttingDown);
        }
        if command == Command::Shutdown {
            queue.stopping = true;
        }
        queue.commands.push_back(command);
        // Under the queue lock: the player thread clears engine_alive under
        // the same lock before it destroys the engine.
        if queue.engine_alive {
            self.shared.engine.wakeup();
        }
        Ok(())
    }

    /// Opens a book. `request_id` comes back in its book-loaded or load-failed event.
    pub fn open(&self, path: &str, request_id: u64) -> Result<(), ErrorCode> {
        if path.is_empty() || path.contains('\0') {
            return Err(ErrorCode::InvalidArgument);
        }
        self.post(Command::Open { path: path.to_owned(), request_id })
    }

    pub fn close_book(&self) -> Result<(), ErrorCode> {
        self.post(Command::Close)
    }

    pub fn play(&self) -> Result<(), ErrorCode> {
        self.post(Command::Play)
    }

    pub fn pause(&self) -> Result<(), ErrorCode> {
        self.post(Command::Pause)
    }

    pub fn seek_absolute(&self, seconds: f64) -> Result<(), ErrorCode> {
        if !seconds.is_finite() {
            return Err(ErrorCode::InvalidArgument);
        }
        self.post(Command::SeekAbsolute(seconds))
    }

    pub fn seek_relative(&self, seconds: f64) -> Result<(), ErrorCode> {
        if !seconds.is_finite() {
            return Err(ErrorCode::InvalidArgument);
        }
        self.post(Command::SeekRelative(seconds))
    }

    pub fn select_chapter(&self, index: i32) -> Result<(), ErrorCode> {
        if index < 0 {
            return Err(ErrorCode::InvalidArgument);
        }
        self.post(Command::ChapterSelect(index))
    }

    pub fn next_chapter(&self) -> Result<(), ErrorCode> {
        self.post(Command::ChapterNext)
    }

    pub fn previous_chapter(&self) -> Result<(), ErrorCode> {
        self.post(Command::ChapterPrevious)
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), ErrorCode> {
        if !volume.is_finite() {
            return Err(ErrorCode::InvalidArgument);
        }
        self.post(Command::Volume(volume))
    }

    pub fn set_mute(&self, mute: bool) -> Result<(), ErrorCode> {
        self.post(Command::Mute(mute))
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&lock(&self.shared.snapshot))
    }

    /// Saves the position, stops the engine and closes the store. No event is
    /// raised after a successful return. `None` waits without limit. On
    /// timeout the position is already saved; the process may simply exit.
    pub fn shutdown(&self, timeout: Option<Duration>) -> Result<(), ErrorCode> {
        if std::thread::current().id() == self.thread_id {
            return Err(ErrorCode::InvalidState);
        }
        let _ = self.post(Command::Shutdown);
        let done = lock(&self.shared.done);
        let done = match timeout {
            None => self.shared.done_signal.wait_while(done, |d| !*d).unwrap_or_else(|e| e.into_inner()),
            Some(t) => {
                let (done, _) =
                    self.shared.done_signal.wait_timeout_while(done, t, |d| !*d).unwrap_or_else(|e| e.into_inner());
                done
            }
        };
        if !*done {
            return Err(ErrorCode::Timeout);
        }
        drop(done);
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
        self.joined.store(true, Ordering::Release);
        Ok(())
    }

    /// The player thread has finished and been joined.
    pub fn is_stopped(&self) -> bool {
        self.joined.load(Ordering::Acquire)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // A thread that does not stop in time is left running detached; it
        // owns everything it still uses.
        if !self.is_stopped() {
            let _ = self.shutdown(Some(Duration::from_secs(5)));
        }
    }
}

fn since(time: Option<Instant>, now: Instant) -> Duration {
    time.map_or(Duration::MAX, |t| now.saturating_duration_since(t))
}

struct Worker {
    shared: Arc<Shared>,
    store: Store,
    sink: Box<dyn EventSink>,
    save_interval: Duration,

    quit: bool,
    state: State,
    generation: u64,
    request_id: u64,
    error: ErrorCode,
    engine_error: i32,
    book: Option<Arc<BookInfo>>,
    /// Store row captured at open; 0 means do not save.
    book_id: i64,
    /// Playlist entry of the current book.
    entry: i64,
    /// Playlist entry the engine is reporting on.
    active_entry: i64,
    /// Saving allowed: current book, first restart seen.
    armed: bool,
    /// Something moved the position since load.
    touched: bool,
    was_finished: bool,
    paused: bool,
    pending_play: bool,
    pending_seek: Option<f64>,
    duration: f64,
    engine_position: f64,
    has_engine_position: bool,
    /// The logical position.
    position: f64,
    chapter: i32,
    /// Last seek target. Until the engine reports a position at or past it,
    /// the logical position is the target, never the slightly earlier value
    /// the engine reports after a seek.
    target: Option<f64>,
    /// Reply id of the latest seek; never reused.
    seek_serial: u64,
    /// The engine has taken the latest seek.
    target_accepted: bool,
    /// And has restarted playback after it.
    target_restarted: bool,
    target_time: Option<Instant>,
    eof: bool,
    position_event_value: f64,
    position_event_time: Option<Instant>,
    volume: f64,
    mute: bool,
    volume_dirty: bool,
    volume_time: Option<Instant>,
    save_failed: bool,
    last_book: String,
    saved_position: f64,
    save_time: Option<Instant>,
    save_due: Option<Instant>,
}

impl Worker {
    fn new(
        shared: Arc<Shared>,
        store: Store,
        sink: Box<dyn EventSink>,
        save_interval: Duration,
        volume: f64,
        mute: bool,
        last_book: String,
    ) -> Worker {
        Worker {
            shared,
            store,
            sink,
            save_interval,
            quit: false,
            state: State::Idle,
            generation: 0,
            request_id: 0,
            error: ErrorCode::None,
            engine_error: 0,
            book: None,
            book_id: 0,
            entry: 0,
            active_entry: 0,
            armed: false,
            touched: false,
            was_finished: false,
            paused: true,
            pending_play: false,
            pending_seek: None,
            duration: 0.0,
            engine_position: 0.0,
            has_engine_position: false,
            position: 0.0,
            chapter: -1,
            target: None,
            seek_serial: 1000,
            target_accepted: false,
            target_restarted: false,
            target_time: None,
            eof: false,
            position_event_value: -1.0,
            position_event_time: None,
            volume,
            mute,
            volume_dirty: false,
            volume_time: None,
            save_failed: false,
            last_book,
            saved_position: -1.0,
            save_time: None,
            save_due: None,
        }
    }

    fn engine(&self) -> &Engine {
        &self.shared.engine
    }

    fn log(&mut self, level: LogLevel, message: &str) {
        self.sink.log(level, message);
    }

    fn run(mut self) {
        let outcome = catch_unwind(AssertUnwindSafe(|| self.run_loop()));
        match outcome {
            // The position is made durable before the engine is touched, so a
            // slow or hung audio device cannot lose it.
            Ok(()) => {
                self.save_now();
                self.flush_volume();
            }
            // A panic is not a rollback: the state may be half-changed, so
            // the last position saved before it is kept as it is.
            Err(_) => self.log(LogLevel::Error, "player thread stopped after an internal error"),
        }
        {
            let mut queue = lock(&self.shared.queue);
            queue.stopping = true;
            queue.engine_alive = false;
        }
        self.engine().terminate();
        self.store.close();
        *lock(&self.shared.done) = true;
        self.shared.done_signal.notify_all();
    }

    fn run_loop(&mut self) {
        while !self.quit {
            let event = self.engine().wait_event(0.1);
            if event != Event::None {
                self.handle_event(event);
            }
            loop {
                let Some(command) = lock(&self.shared.queue).commands.pop_front() else { break };
                self.handle_command(command);
                if self.quit {
                    break;
                }
            }
            if !self.quit {
                self.tick();
            }
        }
    }

    fn current(&self) -> bool {
        self.entry != 0 && self.active_entry == self.entry
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::StartFile { entry } => self.active_entry = entry,
            Event::EndFile { entry, reason, error } => {
                if entry == self.active_entry {
                    self.active_entry = 0;
                }
                if entry != self.entry || self.entry == 0 {
                    return;
                }
                // With keep-open the current book only ends through a
                // failure. Whatever was saved last stays.
                self.entry = 0;
                let code = if reason == END_FILE_REASON_ERROR { ErrorCode::Engine } else { ErrorCode::Io };
                self.fail(code, error);
            }
            Event::FileLoaded => {
                if self.current() {
                    self.read_book_info();
                }
            }
            Event::PlaybackRestart => {
                if !self.current() {
                    return;
                }
                if !self.armed {
                    self.arm();
                    return;
                }
                // A restart counts for the current target only once the
                // engine has taken the latest seek; an earlier one belongs to
                // a seek that has since been replaced.
                if self.target.is_some() && self.target_accepted && !self.target_restarted {
                    self.target_restarted = true;
                    self.target_time = Some(Instant::now());
                }
                self.check_end_of_file();
            }
            Event::CommandReply { reply, error } => {
                if reply != self.seek_serial || self.target.is_none() {
                    return;
                }
                if error < 0 {
                    let message = format!("seek refused: {}", self.engine().error_string(error));
                    self.log(LogLevel::Warning, &message);
                    self.target = None;
                    self.update_position(true);
                    self.save_due = Some(Instant::now());
                } else {
                    self.target_accepted = true;
                }
            }
            Event::PropertyChange { reply, value } => self.handle_property(reply, value),
            Event::QueueOverflow => self.log(LogLevel::Warning, "engine event queue overflow"),
            Event::None | Event::Other => {}
        }
    }

    fn handle_property(&mut self, reply: u64, value: Value) {
        // Volume belongs to the player, not to a file.
        if reply == PROP_VOLUME || reply == PROP_MUTE {
            match (reply, value) {
                (PROP_VOLUME, Value::Double(volume)) if volume != self.volume => self.volume = volume,
                (PROP_MUTE, Value::Flag(mute)) if mute != self.mute => self.mute = mute,
                _ => return,
            }
            self.volume_dirty = true;
            self.volume_time = Some(Instant::now());
            self.publish();
            self.emit(EventKind::VolumeChanged, 0);
            return;
        }

        // Everything else describes the file the engine is on. Reports about
        // a book that is no longer current are dropped.
        if !self.current() {
            return;
        }
        match reply {
            PROP_TIME => {
                // An unavailable position is never used and never saved.
                self.has_engine_position = matches!(value, Value::Double(_));
                if let Value::Double(position) = value {
                    self.engine_position = position;
                    if self.armed {
                        self.update_position(false);
                    }
                }
            }
            PROP_DURATION => {
                if let Value::Double(duration) = value {
                    self.duration = duration;
                }
            }
            PROP_PAUSE => {
                let Value::Flag(paused) = value else { return };
                self.paused = paused;
                if !self.armed || !matches!(self.state, State::Paused | State::Playing) {
                    return;
                }
                if !paused {
                    self.touched = true;
                }
                self.set_state(if paused { State::Paused } else { State::Playing });
                if paused {
                    self.save_now();
                }
            }
            PROP_EOF => {
                self.eof = value == Value::Flag(true);
                if self.armed {
                    self.check_end_of_file();
                }
            }
            _ => {}
        }
    }

    fn handle_command(&mut self, command: Command) {
        let playable = self.armed && self.state != State::Failed;
        match command {
            Command::Open { path, request_id } => self.open_book(path, request_id),
            Command::Close => self.close_book(),
            Command::Shutdown => self.quit = true,
            Command::Play | Command::Pause => {
                let play = command == Command::Play;
                if self.state == State::Loading {
                    self.pending_play = play;
                } else if playable && self.state != State::Finished {
                    self.engine().set_flag_async("pause", !play);
                }
            }
            Command::SeekAbsolute(seconds) => {
                if self.state == State::Loading {
                    // Folded into the start: applied after the first restart.
                    self.pending_seek = Some(seconds);
                } else if playable {
                    self.seek_to(seconds);
                }
            }
            Command::SeekRelative(seconds) => {
                if self.state == State::Loading {
                    self.pending_seek = Some(self.pending_seek.unwrap_or(self.position) + seconds);
                } else if playable {
                    self.seek_to(self.position + seconds);
                }
            }
            Command::ChapterSelect(_) | Command::ChapterNext | Command::ChapterPrevious => {
                let Some(book) = self.book.clone().filter(|_| playable) else { return };
                let before = self.seek_serial;
                let chapters = &book.chapters;
                let count = chapters.len() as i32;
                match command {
                    Command::ChapterSelect(index) => {
                        if index >= 0 && index < count {
                            self.seek_to(chapters[index as usize].start);
                        }
                    }
                    Command::ChapterNext => {
                        // Next on the last chapter does nothing.
                        if self.chapter + 1 < count {
                            self.seek_to(chapters[(self.chapter + 1) as usize].start);
                        }
                    }
                    _ => {
                        if self.chapter <= 0 {
                            self.seek_to(if self.chapter == 0 { chapters[0].start } else { 0.0 });
                        } else if self.position - chapters[self.chapter as usize].start > PREVIOUS_THRESHOLD {
                            self.seek_to(chapters[self.chapter as usize].start);
                        } else {
                            self.seek_to(chapters[(self.chapter - 1) as usize].start);
                        }
                    }
                }
                // A chapter jump is a single deliberate act, unlike a slider
                // drag, so it is saved at once and not after the debounce.
                if self.seek_serial != before {
                    self.save_now();
                }
            }
            Command::Volume(volume) => self.engine().set_double_async("volume", volume.clamp(0.0, 100.0)),
            Command::Mute(mute) => self.engine().set_flag_async("mute", mute),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.armed && self.target.is_some() {
            self.update_position(false);
        }
        let due = self.save_due.is_some_and(|due| now >= due);
        let periodic = since(self.save_time, now) >= self.save_interval
            && ((self.state == State::Playing && self.position != self.saved_position) || self.save_failed);
        if due || periodic {
            self.save_now();
        }
        if self.volume_dirty && since(self.volume_time, now) >= VOLUME_SAVE_DEBOUNCE {
            self.flush_volume();
        }
    }

    fn open_book(&mut self, path: String, request_id: u64) {
        // The old book is saved here, on this thread, before the engine is
        // told anything: loadfile returns before the old file has stopped.
        self.save_now();

        self.generation += 1;
        self.request_id = request_id;
        self.armed = false;
        self.touched = false;
        self.was_finished = false;
        self.pending_play = false;
        self.has_engine_position = false;
        self.pending_seek = None;
        self.target = None;
        self.save_due = None;
        self.book_id = 0;
        self.entry = 0;
        self.error = ErrorCode::None;
        self.engine_error = 0;
        self.duration = 0.0;
        self.position = 0.0;
        self.saved_position = -1.0;
        self.chapter = -1;
        self.paused = true;
        self.eof = false;

        let metadata = std::fs::metadata(&path).ok().filter(|m| !m.is_dir());
        let file_size = metadata.as_ref().map_or(0, |m| m.len() as i64);
        self.book = Some(Arc::new(BookInfo { path: path.clone(), file_size, ..BookInfo::default() }));
        self.set_state(State::Loading);

        if metadata.is_none() {
            self.engine().command_async(0, &["stop"]);
            if let Err(e) = self.store.mark_missing(&path) {
                self.log(LogLevel::Warning, &format!("store: {e}"));
            }
            self.error = ErrorCode::NotFound;
            self.set_state(State::Missing);
            self.emit(EventKind::LoadFailed, self.request_id);
            return;
        }

        let mut start = 0.0;
        match self.store.find_or_create(&path, file_size) {
            Ok(record) => {
                self.book_id = record.id;
                self.was_finished = record.finished;
                // A finished book starts again from the beginning.
                if !record.finished {
                    start = record.position_ms as f64 / 1000.0;
                }
                if self.save_failed {
                    self.save_failed = false;
                    self.emit(EventKind::SaveRecovered, 0);
                }
            }
            Err(e) => {
                self.log(LogLevel::Error, &format!("store: {e}"));
                if !self.save_failed {
                    self.save_failed = true;
                    self.error = ErrorCode::Store;
                    self.emit(EventKind::SaveFailed, 0);
                    self.error = ErrorCode::None;
                }
            }
        }
        self.position = start;
        self.saved_position = start;

        // The engine starts the file at the saved position, paused. There is
        // no load-then-seek, so nothing plays from zero.
        let options = format!("start={start:.3},pause=yes");
        match self.engine().command_ret(&["loadfile", &path, "replace", "-1", &options]) {
            Err(code) => {
                self.fail(ErrorCode::Engine, code);
                return;
            }
            Ok(reply) => {
                if let Some(Node::Int64(id)) = reply.get("playlist_entry_id") {
                    self.entry = *id;
                }
            }
        }
        self.target = Some(start);
        self.target_accepted = false;
        self.target_restarted = false;
        self.publish();
    }

    fn close_book(&mut self) {
        self.save_now();
        self.generation += 1;
        self.armed = false;
        self.book_id = 0;
        self.entry = 0;
        self.book = None;
        self.target = None;
        self.save_due = None;
        self.error = ErrorCode::None;
        self.engine_error = 0;
        self.duration = 0.0;
        self.position = 0.0;
        self.chapter = -1;
        self.engine().command_async(0, &["stop"]);
        self.set_state(State::Idle);
        self.publish();
    }

    fn read_book_info(&mut self) {
        let engine = &self.shared.engine;
        let mut book = self.book.as_deref().cloned().unwrap_or_default();
        if let Some(duration) = engine.get_double("duration") {
            self.duration = duration;
        }
        book.title = dedupe_title(&engine.get_string("media-title"));
        book.author = engine.get_string("metadata/by-key/ARTIST");
        book.comment = engine.get_string("metadata/by-key/COMMENT");
        book.codec = engine.get_string("audio-codec-name");

        book.chapters.clear();
        if let Some(Node::Array(items)) = engine.get_node("chapter-list") {
            for item in &items {
                let mut chapter = Chapter::default();
                if let Some(Node::String(title)) = item.get("title") {
                    chapter.title = trim_chapter_title(title);
                }
                if let Some(Node::Double(time)) = item.get("time") {
                    chapter.start = *time;
                }
                book.chapters.push(chapter);
            }
        }
        // A book without chapters gets one that spans it.
        if book.chapters.is_empty() {
            book.chapters.push(Chapter { title: book.title.clone(), start: 0.0, end: 0.0 });
        }
        // The engine gives no end times: a chapter ends where the next starts.
        let starts: Vec<f64> = book.chapters.iter().skip(1).map(|c| c.start).collect();
        for (i, chapter) in book.chapters.iter_mut().enumerate() {
            chapter.end = starts.get(i).copied().unwrap_or(self.duration);
        }
        if self.book_id != 0
            && !book.title.is_empty()
            && let Err(e) = self.store.set_title(self.book_id, &book.title)
        {
            self.log(LogLevel::Warning, &format!("store: {e}"));
        }
        self.book = Some(Arc::new(book));
    }

    /// First restart of the current book: the engine is at the start
    /// position. From here on the position belongs to this book and may be saved.
    fn arm(&mut self) {
        self.armed = true;
        self.target_accepted = true;
        self.target_restarted = true;
        self.target_time = Some(Instant::now());
        if self.duration > 0.0 && self.position > self.duration {
            self.position = self.duration;
        }
        self.chapter = self.chapter_at(self.position);
        self.set_state(State::Paused);
        self.publish();
        self.emit(EventKind::BookLoaded, self.request_id);

        let path = self.book.as_ref().map(|b| b.path.clone()).unwrap_or_default();
        if self.last_book != path {
            if let Err(e) = self.store.set_setting("last_book", &path) {
                self.log(LogLevel::Warning, &format!("store: {e}"));
            }
            self.last_book = path;
            self.publish();
        }

        // Read once here: starting at or past the end may not raise a change
        // notification. The engine has just restarted, so this does not wait.
        if let Some(eof) = self.engine().get_flag("eof-reached") {
            self.eof = eof;
        }
        self.check_end_of_file();
        if self.state != State::Paused {
            return;
        }
        if let Some(seek) = self.pending_seek.take() {
            self.seek_to(seek);
        }
        if std::mem::take(&mut self.pending_play) {
            self.engine().set_flag_async("pause", false);
        }
    }

    fn check_end_of_file(&mut self) {
        if !self.eof {
            // Seeking away from the end of a finished book.
            if self.state == State::Finished {
                self.set_state(if self.paused { State::Paused } else { State::Playing });
            }
            return;
        }
        if matches!(self.state, State::Finished | State::Failed) {
            return;
        }
        // End of file far from the end means a truncated or vanished file,
        // not a finished book. The record keeps its last saved position.
        let reached = if self.has_engine_position { self.engine_position.max(self.position) } else { self.position };
        if self.duration > 0.0 && reached >= self.duration - END_TOLERANCE {
            self.finish();
        } else {
            self.fail(ErrorCode::Io, 0);
        }
    }

    fn finish(&mut self) {
        self.position = self.duration;
        self.target = None;
        self.chapter = self.chapter_at(self.position);
        self.touched = true;
        self.set_state(State::Finished);
        self.save_now();
        self.emit(EventKind::Finished, 0);
    }

    fn fail(&mut self, error: ErrorCode, engine_error: i32) {
        let loading = !self.armed;
        self.error = error;
        self.engine_error = engine_error;
        self.save_due = None;
        self.set_state(State::Failed);
        if loading {
            self.emit(EventKind::LoadFailed, self.request_id);
        }
    }

    fn seek_to(&mut self, target: f64) {
        let target = target.clamp(0.0, if self.duration > 0.0 { self.duration } else { target.max(0.0) });
        // The logical position moves first, so a snapshot shows the new
        // place even while the engine is still busy getting there.
        self.target = Some(target);
        self.target_accepted = false;
        self.target_restarted = false;
        self.touched = true;
        // Seeking back from the end of a finished book reopens it.
        if self.state == State::Finished && target < self.duration - END_TOLERANCE {
            self.eof = false;
            self.set_state(if self.paused { State::Paused } else { State::Playing });
        }
        self.update_position(true);
        // The target is saved without waiting for the engine: if the process
        // dies mid-seek, the book resumes where the listener asked to be.
        self.save_due = Some(Instant::now() + SEEK_SAVE_DEBOUNCE);

        self.seek_serial += 1;
        let seconds = format!("{target:.6}");
        self.engine().command_async(self.seek_serial, &["seek", &seconds, "absolute+exact"]);
    }

    fn chapter_at(&self, position: f64) -> i32 {
        let Some(book) = &self.book else { return -1 };
        // Positions are stored in whole milliseconds, rounded down, so a
        // position saved exactly on a chapter start comes back up to 1 ms before it.
        book.chapters.iter().take_while(|c| c.start <= position + 0.001).count() as i32 - 1
    }

    /// Derives the logical position from the engine's report and the last
    /// seek target. The engine can report a position slightly before a seek
    /// target, and before the seek has run it still reports the old
    /// position; in both cases the logical position is the target.
    fn update_position(&mut self, force_event: bool) {
        let now = Instant::now();
        let mut position = if self.has_engine_position { self.engine_position } else { self.position };
        if let Some(target) = self.target {
            let near_target = position >= target - 1.0 && position <= target + END_TOLERANCE;
            if !self.has_engine_position || !self.target_restarted {
                // Until the engine has restarted after the seek it still
                // reports where it was, however long a slow disk makes that take.
                position = target;
            } else if near_target {
                // Paused at a target the engine may report at it and then
                // just before it, so the target is dropped only once well past it.
                if position > target + TARGET_RELEASE {
                    self.target = None;
                } else {
                    position = position.max(target);
                }
            } else if since(self.target_time, now) < SEEK_SETTLE_TIMEOUT {
                position = target;
            } else {
                self.target = None; // the engine settled elsewhere; believe it
            }
        }
        if self.duration > 0.0 {
            position = position.min(self.duration);
        }
        self.position = position.max(0.0);

        let chapter = self.chapter_at(self.position);
        let chapter_changed = chapter != self.chapter;
        self.chapter = chapter;

        if !force_event
            && !chapter_changed
            && (self.position == self.position_event_value
                || since(self.position_event_time, now) < POSITION_EVENT_INTERVAL)
        {
            return;
        }
        self.position_event_time = Some(now);
        self.position_event_value = self.position;
        self.publish();
        if chapter_changed {
            self.emit(EventKind::ChapterChanged, 0);
        }
        self.emit(EventKind::Position, 0);
    }

    /// Every save passes the same guards: the position belongs to the
    /// current book, it is known, it is in range, and something has moved it since load.
    fn save_now(&mut self) {
        self.save_due = None;
        if !self.armed || !self.touched || self.book_id == 0 || self.state == State::Failed {
            return;
        }
        if self.position < 0.0 || (self.duration > 0.0 && self.position > self.duration + 1.0) {
            return;
        }
        let finished = self.state == State::Finished;
        self.save_time = Some(Instant::now());
        // Rounded down: a resume may repeat a moment, it never skips one.
        let result = self.store.save_position(
            self.book_id,
            (self.position * 1000.0).floor() as i64,
            (self.duration * 1000.0).round() as i64,
            finished,
        );
        let ok = result.is_ok();
        match result {
            Ok(()) => {
                self.saved_position = self.position;
                if finished {
                    self.was_finished = true;
                }
            }
            Err(e) => self.log(LogLevel::Error, &format!("store: {e}")),
        }
        if ok == self.save_failed {
            self.save_failed = !ok;
            let previous = self.error;
            if !ok {
                self.error = ErrorCode::Store;
            }
            self.publish();
            self.emit(if ok { EventKind::SaveRecovered } else { EventKind::SaveFailed }, 0);
            self.error = previous;
        }
    }

    fn flush_volume(&mut self) {
        if !std::mem::take(&mut self.volume_dirty) {
            return;
        }
        let volume = format!("{:.1}", self.volume);
        let result = self
            .store
            .set_setting("volume", &volume)
            .and_then(|()| self.store.set_setting("mute", if self.mute { "1" } else { "0" }));
        if let Err(e) = result {
            self.log(LogLevel::Warning, &format!("store: {e}"));
        }
    }

    fn set_state(&mut self, state: State) {
        if state == self.state {
            return;
        }
        self.state = state;
        self.publish();
        self.emit(EventKind::StateChanged, 0);
    }

    fn publish(&self) {
        let next = Arc::new(Snapshot {
            state: self.state,
            generation: self.generation,
            error: self.error,
            engine_error: self.engine_error,
            chapter: self.chapter,
            position: self.position,
            duration: self.duration,
            volume: self.volume,
            mute: self.mute,
            save_failed: self.save_failed,
            was_finished: self.was_finished,
            last_book: self.last_book.clone(),
            book: self.book.clone(),
        });
        *lock(&self.shared.snapshot) = next;
    }

    fn emit(&mut self, kind: EventKind, request_id: u64) {
        let event = PlayerEvent {
            kind,
            generation: self.generation,
            request_id,
            state: self.state,
            error: self.error,
            engine_error: self.engine_error,
            chapter: self.chapter,
            position: self.position,
            duration: self.duration,
            volume: self.volume,
            mute: self.mute,
        };
        self.sink.event(&event);
    }
}
