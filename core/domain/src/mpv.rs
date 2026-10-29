//! libmpv, loaded at run time from an absolute path. The official Windows
//! package ships only a MinGW import library, so the client API is reached
//! through function pointers resolved from the DLL. Types and constants follow
//! `mpv/client.h` (client API 2.5); only what the player uses is declared.
//!
//! The mpv client API is thread-safe: every function may be called from any
//! thread, except that `mpv_wait_event` must have one caller at a time. The
//! player calls everything except [`Engine::wakeup`] from its own thread.

use std::ffi::{CStr, CString, c_char, c_double, c_int, c_ulong, c_void};
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
pub struct MpvHandle {
    _private: [u8; 0],
}

const FORMAT_NONE: c_int = 0;
const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
const FORMAT_NODE: c_int = 6;
const FORMAT_NODE_ARRAY: c_int = 7;
const FORMAT_NODE_MAP: c_int = 8;

const EVENT_NONE: c_int = 0;
const EVENT_COMMAND_REPLY: c_int = 5;
const EVENT_START_FILE: c_int = 6;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PLAYBACK_RESTART: c_int = 21;
const EVENT_PROPERTY_CHANGE: c_int = 22;
const EVENT_QUEUE_OVERFLOW: c_int = 24;

pub const END_FILE_REASON_ERROR: i32 = 4;

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct RawStartFile {
    playlist_entry_id: i64,
}

/// The leading fields of `mpv_event_end_file`; the struct is only read through a pointer.
#[repr(C)]
struct RawEndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
union RawNodeValue {
    string: *mut c_char,
    flag: c_int,
    int64: i64,
    double: c_double,
    list: *mut RawNodeList,
}

#[repr(C)]
struct RawNode {
    u: RawNodeValue,
    format: c_int,
}

#[repr(C)]
struct RawNodeList {
    num: c_int,
    values: *mut RawNode,
    keys: *mut *mut c_char,
}

/// An owned copy of an `mpv_node`.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    None,
    String(String),
    Flag(bool),
    Int64(i64),
    Double(f64),
    Array(Vec<Node>),
    Map(Vec<(String, Node)>),
    Other,
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// A property value as reported in a change notification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// The property is unavailable (no file, or not yet known).
    None,
    Double(f64),
    Flag(bool),
    Other,
}

/// An owned copy of an `mpv_event`, made before the next wait.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    None,
    StartFile { entry: i64 },
    EndFile { entry: i64, reason: i32, error: i32 },
    FileLoaded,
    PlaybackRestart,
    CommandReply { reply: u64, error: i32 },
    PropertyChange { reply: u64, value: Value },
    QueueOverflow,
    Other,
}

type Create = unsafe extern "C" fn() -> *mut MpvHandle;
type Initialize = unsafe extern "C" fn(*mut MpvHandle) -> c_int;
type TerminateDestroy = unsafe extern "C" fn(*mut MpvHandle);
type SetOptionString = unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int;
type CommandRet = unsafe extern "C" fn(*mut MpvHandle, *mut *const c_char, *mut RawNode) -> c_int;
type CommandAsync = unsafe extern "C" fn(*mut MpvHandle, u64, *mut *const c_char) -> c_int;
type GetProperty = unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int;
type SetPropertyAsync = unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int, *mut c_void) -> c_int;
type GetPropertyString = unsafe extern "C" fn(*mut MpvHandle, *const c_char) -> *mut c_char;
type ObserveProperty = unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int) -> c_int;
type WaitEvent = unsafe extern "C" fn(*mut MpvHandle, c_double) -> *mut RawEvent;
type Wakeup = unsafe extern "C" fn(*mut MpvHandle);
type ErrorString = unsafe extern "C" fn(c_int) -> *const c_char;
type Free = unsafe extern "C" fn(*mut c_void);
type FreeNodeContents = unsafe extern "C" fn(*mut RawNode);
type ClientApiVersion = unsafe extern "C" fn() -> c_ulong;

struct Api {
    create: Create,
    initialize: Initialize,
    terminate_destroy: TerminateDestroy,
    set_option_string: SetOptionString,
    command_ret: CommandRet,
    command_async: CommandAsync,
    get_property: GetProperty,
    set_property_async: SetPropertyAsync,
    get_property_string: GetPropertyString,
    observe_property: ObserveProperty,
    wait_event: WaitEvent,
    wakeup: Wakeup,
    error_string: ErrorString,
    free: Free,
    free_node_contents: FreeNodeContents,
    // Kept last so the pointers above are dropped before the DLL is.
    _library: libloading::Library,
}

/// Client API major version this binding is written against.
const API_MAJOR: c_ulong = 2;

impl Api {
    fn load(path: &Path) -> Result<Api, String> {
        // An absolute path with the altered search order: a stray copy of
        // libmpv elsewhere on the system is never picked up.
        // SAFETY: loading libmpv runs only its DLL initialization, which has no preconditions.
        let library = unsafe {
            libloading::os::windows::Library::load_with_flags(
                path,
                libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        }
        .map_err(|e| format!("cannot load {}: {e}", path.display()))?;
        let library = libloading::Library::from(library);
        macro_rules! symbol {
            ($name:literal) => {
                // SAFETY: the declared type matches the prototype in mpv/client.h.
                *unsafe { library.get(concat!("mpv_", $name, "\0").as_bytes()) }
                    .map_err(|_| format!("{} has no export mpv_{}", path.display(), $name))?
            };
        }
        let version: ClientApiVersion = symbol!("client_api_version");
        // SAFETY: mpv_client_api_version takes no arguments and only returns a constant.
        let major = unsafe { version() } >> 16;
        if major != API_MAJOR {
            return Err(format!("{} has client API {major}, expected {API_MAJOR}", path.display()));
        }
        Ok(Api {
            create: symbol!("create"),
            initialize: symbol!("initialize"),
            terminate_destroy: symbol!("terminate_destroy"),
            set_option_string: symbol!("set_option_string"),
            command_ret: symbol!("command_ret"),
            command_async: symbol!("command_async"),
            get_property: symbol!("get_property"),
            set_property_async: symbol!("set_property_async"),
            get_property_string: symbol!("get_property_string"),
            observe_property: symbol!("observe_property"),
            wait_event: symbol!("wait_event"),
            wakeup: symbol!("wakeup"),
            error_string: symbol!("error_string"),
            free: symbol!("free"),
            free_node_contents: symbol!("free_node_contents"),
            _library: library,
        })
    }
}

fn c_string(text: &str) -> CString {
    // Callers pass validated text; an interior NUL would only come from a bug.
    CString::new(text).unwrap_or_default()
}

/// Copies an mpv-owned C string.
///
/// # Safety
/// `text` must be null or a valid NUL-terminated string for the duration of the call.
unsafe fn copy_string(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: guaranteed by the caller.
    unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
}

/// Copies an mpv node tree.
///
/// # Safety
/// `node` must be a valid node as returned by mpv, not yet freed.
unsafe fn copy_node(node: &RawNode) -> Node {
    // SAFETY (all arms): the union member read is the one `format` says is valid,
    // and mpv guarantees list pointers and lengths are consistent.
    unsafe {
        match node.format {
            FORMAT_NONE => Node::None,
            FORMAT_STRING => Node::String(copy_string(node.u.string)),
            FORMAT_FLAG => Node::Flag(node.u.flag != 0),
            FORMAT_INT64 => Node::Int64(node.u.int64),
            FORMAT_DOUBLE => Node::Double(node.u.double),
            FORMAT_NODE_ARRAY | FORMAT_NODE_MAP => {
                let list = node.u.list;
                if list.is_null() {
                    return Node::Other;
                }
                let count = usize::try_from((*list).num).unwrap_or(0);
                let values = (0..count).map(|i| copy_node(&*(*list).values.add(i)));
                if node.format == FORMAT_NODE_ARRAY {
                    Node::Array(values.collect())
                } else {
                    let keys = (0..count).map(|i| copy_string(*(*list).keys.add(i)));
                    Node::Map(keys.zip(values).collect())
                }
            }
            _ => Node::Other,
        }
    }
}

/// One mpv instance. After [`Engine::terminate`] only `wakeup`, `wait_event`
/// and `terminate` may be called; they then do nothing.
pub struct Engine {
    api: Api,
    handle: AtomicPtr<MpvHandle>,
}

// SAFETY: the mpv client API is thread-safe for a handle (see the module
// documentation); the handle is only invalidated by `terminate`, which swaps
// it out atomically first, and callers stop using `wakeup` before then.
unsafe impl Send for Engine {}
// SAFETY: as above.
unsafe impl Sync for Engine {}

impl Engine {
    /// Loads libmpv from `dll` and creates an uninitialized instance.
    pub fn create(dll: &Path) -> Result<Engine, String> {
        let api = Api::load(dll)?;
        // SAFETY: mpv_create has no preconditions; it fails only if LC_NUMERIC is not "C",
        // which Rust and .NET processes do not change.
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err("mpv_create failed; LC_NUMERIC must be \"C\"".into());
        }
        Ok(Engine { api, handle: AtomicPtr::new(handle) })
    }

    fn handle(&self) -> *mut MpvHandle {
        self.handle.load(Ordering::Acquire)
    }

    pub fn error_string(&self, code: i32) -> String {
        // SAFETY: mpv_error_string accepts any value and returns a static string.
        unsafe { copy_string((self.api.error_string)(code)) }
    }

    fn check(&self, code: c_int) -> Result<(), i32> {
        if code < 0 { Err(code) } else { Ok(()) }
    }

    pub fn set_option(&self, name: &str, value: &str) -> Result<(), i32> {
        let (name, value) = (c_string(name), c_string(value));
        // SAFETY: valid handle and NUL-terminated strings borrowed for the call.
        self.check(unsafe { (self.api.set_option_string)(self.handle(), name.as_ptr(), value.as_ptr()) })
    }

    pub fn initialize(&self) -> Result<(), i32> {
        // SAFETY: valid handle.
        self.check(unsafe { (self.api.initialize)(self.handle()) })
    }

    pub fn observe_double(&self, reply: u64, name: &str) {
        self.observe(reply, name, FORMAT_DOUBLE);
    }

    pub fn observe_flag(&self, reply: u64, name: &str) {
        self.observe(reply, name, FORMAT_FLAG);
    }

    fn observe(&self, reply: u64, name: &str, format: c_int) {
        let name = c_string(name);
        // SAFETY: valid handle and string.
        unsafe { (self.api.observe_property)(self.handle(), reply, name.as_ptr(), format) };
    }

    /// Waits up to `timeout` seconds and copies the event out.
    pub fn wait_event(&self, timeout: f64) -> Event {
        let handle = self.handle();
        if handle.is_null() {
            return Event::None;
        }
        // SAFETY: valid handle; only the player thread waits. The returned event and
        // everything it points to stay valid until the next wait, and are copied here.
        unsafe {
            let event = &*(self.api.wait_event)(handle, timeout);
            match event.event_id {
                EVENT_NONE => Event::None,
                EVENT_START_FILE => {
                    Event::StartFile { entry: (*(event.data as *const RawStartFile)).playlist_entry_id }
                }
                EVENT_END_FILE => {
                    let end = &*(event.data as *const RawEndFile);
                    Event::EndFile { entry: end.playlist_entry_id, reason: end.reason, error: end.error }
                }
                EVENT_FILE_LOADED => Event::FileLoaded,
                EVENT_PLAYBACK_RESTART => Event::PlaybackRestart,
                EVENT_COMMAND_REPLY => Event::CommandReply { reply: event.reply_userdata, error: event.error },
                EVENT_PROPERTY_CHANGE => {
                    let property = &*(event.data as *const RawProperty);
                    let value = if property.data.is_null() {
                        Value::None
                    } else {
                        match property.format {
                            FORMAT_NONE => Value::None,
                            FORMAT_DOUBLE => Value::Double(*(property.data as *const f64)),
                            FORMAT_FLAG => Value::Flag(*(property.data as *const c_int) != 0),
                            _ => Value::Other,
                        }
                    };
                    Event::PropertyChange { reply: event.reply_userdata, value }
                }
                EVENT_QUEUE_OVERFLOW => Event::QueueOverflow,
                _ => Event::Other,
            }
        }
    }

    /// Interrupts a [`Engine::wait_event`] in progress. Safe from any thread
    /// until [`Engine::terminate`] begins.
    pub fn wakeup(&self) {
        let handle = self.handle();
        if !handle.is_null() {
            // SAFETY: valid handle.
            unsafe { (self.api.wakeup)(handle) };
        }
    }

    fn argv(args: &[&str]) -> (Vec<CString>, Vec<*const c_char>) {
        let owned: Vec<CString> = args.iter().map(|a| c_string(a)).collect();
        let mut pointers: Vec<*const c_char> = owned.iter().map(|a| a.as_ptr()).collect();
        pointers.push(ptr::null());
        (owned, pointers)
    }

    /// Queues a command without waiting; the outcome arrives as a
    /// [`Event::CommandReply`] carrying `reply` (0 asks for none to matter).
    pub fn command_async(&self, reply: u64, args: &[&str]) {
        let (_owned, mut argv) = Self::argv(args);
        // SAFETY: valid handle and a NULL-terminated array of strings that outlive the call.
        unsafe { (self.api.command_async)(self.handle(), reply, argv.as_mut_ptr()) };
    }

    /// Runs a command and returns its result.
    pub fn command_ret(&self, args: &[&str]) -> Result<Node, i32> {
        let (_owned, mut argv) = Self::argv(args);
        let mut result = RawNode { u: RawNodeValue { int64: 0 }, format: FORMAT_NONE };
        // SAFETY: valid handle, argument array as above, and a zeroed node for the result.
        let code = unsafe { (self.api.command_ret)(self.handle(), argv.as_mut_ptr(), &mut result) };
        self.check(code)?;
        // SAFETY: on success mpv filled `result`; it is copied and then freed once.
        let node = unsafe { copy_node(&result) };
        unsafe { (self.api.free_node_contents)(&mut result) };
        Ok(node)
    }

    pub fn set_flag_async(&self, name: &str, value: bool) {
        let mut flag: c_int = value.into();
        let name = c_string(name);
        // SAFETY: valid handle; mpv copies the value before returning.
        unsafe { (self.api.set_property_async)(self.handle(), 0, name.as_ptr(), FORMAT_FLAG, (&raw mut flag).cast()) };
    }

    pub fn set_double_async(&self, name: &str, value: f64) {
        let mut value = value;
        let name = c_string(name);
        // SAFETY: as for set_flag_async.
        unsafe {
            (self.api.set_property_async)(self.handle(), 0, name.as_ptr(), FORMAT_DOUBLE, (&raw mut value).cast())
        };
    }

    pub fn get_string(&self, name: &str) -> String {
        let name = c_string(name);
        // SAFETY: valid handle; the returned string is copied and freed with mpv_free.
        unsafe {
            let raw = (self.api.get_property_string)(self.handle(), name.as_ptr());
            let text = copy_string(raw);
            if !raw.is_null() {
                (self.api.free)(raw.cast());
            }
            text
        }
    }

    pub fn get_double(&self, name: &str) -> Option<f64> {
        let mut value: f64 = 0.0;
        let name = c_string(name);
        // SAFETY: valid handle and an f64 for MPV_FORMAT_DOUBLE.
        let code =
            unsafe { (self.api.get_property)(self.handle(), name.as_ptr(), FORMAT_DOUBLE, (&raw mut value).cast()) };
        (code >= 0).then_some(value)
    }

    pub fn get_flag(&self, name: &str) -> Option<bool> {
        let mut value: c_int = 0;
        let name = c_string(name);
        // SAFETY: valid handle and a c_int for MPV_FORMAT_FLAG.
        let code =
            unsafe { (self.api.get_property)(self.handle(), name.as_ptr(), FORMAT_FLAG, (&raw mut value).cast()) };
        (code >= 0).then_some(value != 0)
    }

    pub fn get_node(&self, name: &str) -> Option<Node> {
        let mut node = RawNode { u: RawNodeValue { int64: 0 }, format: FORMAT_NONE };
        let name = c_string(name);
        // SAFETY: valid handle and a node for MPV_FORMAT_NODE; copied then freed once.
        unsafe {
            if (self.api.get_property)(self.handle(), name.as_ptr(), FORMAT_NODE, (&raw mut node).cast()) < 0 {
                return None;
            }
            let copy = copy_node(&node);
            (self.api.free_node_contents)(&mut node);
            Some(copy)
        }
    }

    /// Stops playback and destroys the instance. Later calls do nothing.
    pub fn terminate(&self) {
        let handle = self.handle.swap(ptr::null_mut(), Ordering::AcqRel);
        if !handle.is_null() {
            // SAFETY: the handle was valid and is now unreachable through `self`.
            unsafe { (self.api.terminate_destroy)(handle) };
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.terminate();
    }
}
