//! Domain core for Show Player: the playback state machine over libmpv,
//! the resume store and the book library. It has no dependency on any shell;
//! the C ABI facade (`show_player_ffi`) and the console harness are its
//! front ends.

pub mod catalogue;
pub mod library;
pub mod matroska;
pub mod mpv;
pub mod player;
pub mod shows;
pub mod store;
pub mod text;

use std::path::PathBuf;

/// `%LOCALAPPDATA%\show-player`, the folder every release has used.
pub fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(|v| PathBuf::from(v).join("show-player"))
}

/// `libmpv-2.dll` beside the running executable.
pub fn default_engine_path() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|dir| dir.join("libmpv-2.dll"))
}
