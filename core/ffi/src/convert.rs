//! Domain values to ABI messages.

use show_player_core::library::BookEntry;
use show_player_core::player::{ErrorCode, EventKind, PlayerEvent, Snapshot, State};

use crate::messages::{self, ErrorKind, EventType, PlaybackState};

pub fn state(state: State) -> PlaybackState {
    match state {
        State::Idle => PlaybackState::Idle,
        State::Loading => PlaybackState::Loading,
        State::Paused => PlaybackState::Paused,
        State::Playing => PlaybackState::Playing,
        State::Finished => PlaybackState::Finished,
        State::Missing => PlaybackState::Missing,
        State::Failed => PlaybackState::Failed,
    }
}

pub fn error_kind(code: ErrorCode) -> ErrorKind {
    match code {
        ErrorCode::None => ErrorKind::None,
        ErrorCode::InvalidArgument => ErrorKind::InvalidArgument,
        ErrorCode::InvalidState | ErrorCode::Timeout => ErrorKind::InvalidState,
        ErrorCode::NotFound => ErrorKind::NotFound,
        ErrorCode::Io => ErrorKind::Io,
        ErrorCode::Store => ErrorKind::Store,
        ErrorCode::Engine => ErrorKind::Engine,
        ErrorCode::Unsupported => ErrorKind::UnknownCommand,
        ErrorCode::ShuttingDown => ErrorKind::ShuttingDown,
        ErrorCode::Internal => ErrorKind::Internal,
    }
}

fn event_type(kind: EventKind) -> EventType {
    match kind {
        EventKind::StateChanged => EventType::StateChanged,
        EventKind::BookLoaded => EventType::BookLoaded,
        EventKind::LoadFailed => EventType::LoadFailed,
        EventKind::Position => EventType::Position,
        EventKind::ChapterChanged => EventType::ChapterChanged,
        EventKind::Finished => EventType::Finished,
        EventKind::SaveFailed => EventType::SaveFailed,
        EventKind::SaveRecovered => EventType::SaveRecovered,
        EventKind::VolumeChanged => EventType::VolumeChanged,
    }
}

pub fn event_to_message(e: &PlayerEvent) -> messages::Event {
    messages::Event {
        r#type: event_type(e.kind) as i32,
        generation: e.generation,
        request_id: e.request_id,
        state: state(e.state) as i32,
        error: error_kind(e.error) as i32,
        engine_error: e.engine_error,
        chapter: e.chapter,
        position: e.position,
        duration: e.duration,
        volume: e.volume,
        mute: e.mute,
        log_level: 0,
        message: String::new(),
    }
}

pub fn snapshot_to_message(s: &Snapshot, include_chapters: bool) -> messages::Snapshot {
    let book = s.book.as_deref();
    let text = |f: fn(&show_player_core::player::BookInfo) -> &String| book.map(|b| f(b).clone()).unwrap_or_default();
    messages::Snapshot {
        state: state(s.state) as i32,
        generation: s.generation,
        error: error_kind(s.error) as i32,
        engine_error: s.engine_error,
        chapter: s.chapter,
        chapter_count: book.map_or(0, |b| b.chapters.len() as i32),
        position: s.position,
        duration: s.duration,
        volume: s.volume,
        mute: s.mute,
        save_failed: s.save_failed,
        was_finished: s.was_finished,
        file_size: book.map_or(0, |b| b.file_size),
        path: text(|b| &b.path),
        title: text(|b| &b.title),
        author: text(|b| &b.author),
        comment: text(|b| &b.comment),
        codec: text(|b| &b.codec),
        last_book: s.last_book.clone(),
        chapters: match (include_chapters, book) {
            (true, Some(b)) => b
                .chapters
                .iter()
                .map(|c| messages::Chapter { title: c.title.clone(), start: c.start, end: c.end })
                .collect(),
            _ => Vec::new(),
        },
    }
}

pub fn books_to_message(books: &[BookEntry]) -> messages::BookList {
    messages::BookList {
        books: books
            .iter()
            .map(|b| messages::Book {
                path: b.path.clone(),
                title: b.title.clone(),
                position_ms: b.position_ms,
                duration_ms: b.duration_ms,
                finished: b.finished,
                last_played_utc: b.last_played_utc,
                exists: b.exists,
            })
            .collect(),
    }
}
// Show import and catalogue values cross the same generated protobuf boundary.
impl From<show_player_core::shows::Track> for crate::messages::ShowTrack {
    fn from(t: show_player_core::shows::Track) -> Self {
        Self {
            path: t.path,
            title: t.title,
            set: t.set,
            duration: t.duration,
            sample_rate: t.sample_rate,
            channels: t.channels,
            codec: t.codec,
            bit_depth: t.bit_depth,
            sample_format: t.sample_format,
            channel_layout: t.channel_layout,
        }
    }
}
impl From<crate::messages::ShowTrack> for show_player_core::shows::Track {
    fn from(t: crate::messages::ShowTrack) -> Self {
        Self {
            path: t.path,
            title: t.title,
            set: t.set,
            duration: t.duration,
            sample_rate: t.sample_rate,
            channels: t.channels,
            codec: t.codec,
            bit_depth: t.bit_depth,
            sample_format: t.sample_format,
            channel_layout: t.channel_layout,
        }
    }
}
impl From<show_player_core::shows::Draft> for crate::messages::ShowDraft {
    fn from(d: show_player_core::shows::Draft) -> Self {
        Self {
            folder: d.folder,
            artist: d.artist,
            date: d.date,
            venue: d.venue,
            notes_path: d.notes_path,
            notes: d.notes,
            warnings: d.warnings,
            tracks: d.tracks.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<crate::messages::ShowDraft> for show_player_core::shows::Draft {
    fn from(d: crate::messages::ShowDraft) -> Self {
        Self {
            folder: d.folder,
            artist: d.artist,
            date: d.date,
            venue: d.venue,
            notes_path: d.notes_path,
            notes: d.notes,
            warnings: d.warnings,
            tracks: d.tracks.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<show_player_core::shows::Recording> for crate::messages::Recording {
    fn from(r: show_player_core::shows::Recording) -> Self {
        Self { title: r.title, source: r.source, url: r.url, key: r.key, detail: r.detail }
    }
}
impl From<crate::messages::Recording> for show_player_core::shows::Recording {
    fn from(r: crate::messages::Recording) -> Self {
        Self { title: r.title, source: r.source, url: r.url, key: r.key, detail: r.detail }
    }
}
impl From<show_player_core::shows::Job> for crate::messages::ShowJob {
    fn from(j: show_player_core::shows::Job) -> Self {
        Self { state: j.state, phase: j.phase, log: j.log, output: j.output }
    }
}
