//! The books in a folder and the books played recently, with what the store
//! knows about each. Lists never open a book in the engine.

use std::path::Path;

use crate::linking::{self, Evidence};
use crate::matroska;
use crate::store::{BookRow, Reader};

/// File types listed as books. Any type the engine plays can still be opened directly.
pub const BOOK_EXTENSIONS: [&str; 1] = ["mka"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookEntry {
    pub source_summary: String,
    pub credits: String,
    pub source_kind: String,
    pub evidence: String,
    pub modifiers: Vec<String>,
    pub path: String,
    pub title: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub finished: bool,
    /// Seconds since 1970 (UTC); 0 if never played.
    pub last_played_utc: i64,
    /// The file is present.
    pub exists: bool,
    /// BerthaML show and recording IDs, empty when linking found none.
    pub show_id: String,
    pub recording_id: String,
    /// What linking matched.
    pub link: String,
    /// BerthaML performance IDs held by each chapter.
    pub performances: Vec<Vec<String>>,
    /// The show's date as YYYY-MM-DD, from BerthaML or else the book itself; empty if unknown.
    pub date: String,
    /// The date written out, such as "Sunday, May 22, 1977".
    pub show_date: String,
    /// Venue, city and region from BerthaML; empty when the show is not linked.
    pub place: String,
    /// The tour as Jerrybase groups it, such as "Spring 1977 tour"; often empty.
    pub tour: String,
}

fn file_stem(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_owned())
}

/// The best available name: the title read when the book was last opened,
/// then the file's own Matroska title, then the file name.
fn title_of(path: &str, stored: Option<&str>) -> String {
    stored
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .or_else(|| matroska::read_title(Path::new(path)))
        .unwrap_or_else(|| file_stem(path))
}

/// Links a book through the source folder recorded when it was packaged, or else through
/// its own date, title and chapters. Worked out on every list, so it follows corpus changes.
fn link(path: &str, title: &str, stored: Option<String>, database: &Path) -> linking::Link {
    let evidence = stored
        .and_then(|s| serde_json::from_str::<Evidence>(&s).ok())
        .unwrap_or_else(|| linking::mka_evidence(Path::new(path), title));
    linking::link(database, &evidence).unwrap_or_default()
}

/// A book's source profile, with roles from what its notes say each person did.
fn book_profile(notes: &str, title: &str, path: &str) -> crate::catalogue::Profile {
    let (taper, transfer) = crate::catalogue::credited_in(notes);
    crate::catalogue::classify(notes, &format!("{title} {path}"), &taper, &transfer, "", "")
}

fn entry(path: String, row: Option<BookRow>, stored: Option<String>, database: &Path) -> BookEntry {
    let exists = Path::new(&path).is_file();
    let title = title_of(&path, row.as_ref().map(|r| r.title.as_str()));
    let link = link(&path, &title, stored, database);
    let show = (!link.show.is_empty()).then(|| crate::catalogue::show(database, &link.show).ok().flatten()).flatten();
    let date = show
        .as_ref()
        .map(|s| s.summary.date.clone())
        .unwrap_or_else(|| linking::mka_evidence(Path::new(&path), &title).date);
    let (place, tour) = show.map(|s| (s.summary.place, s.summary.tour)).unwrap_or_default();
    let (position_ms, duration_ms, finished, last_played_utc) =
        row.map_or((0, 0, false, 0), |r| (r.position_ms, r.duration_ms, r.finished, r.last_played_utc));
    let notes = matroska::read_source_notes(Path::new(&path));
    let profile = book_profile(&notes, &title, &path);
    BookEntry {
        source_summary: profile.summary(),
        credits: profile.credits,
        source_kind: profile.kind,
        evidence: profile.evidence,
        modifiers: profile.modifiers,
        path,
        title,
        position_ms,
        duration_ms,
        finished,
        last_played_utc,
        exists,
        show_id: link.show,
        recording_id: link.recording,
        link: link.detail,
        performances: link.performances,
        show_date: if date.is_empty() { String::new() } else { crate::report::long_date(&date) },
        date,
        place,
        tour,
    }
}

/// The books directly in `folder`, by title. A store that cannot be read
/// leaves the progress fields empty rather than failing the list.
pub fn list_folder(reader: &mut Reader, folder: &Path, database: &Path) -> Result<Vec<BookEntry>, String> {
    let entries = std::fs::read_dir(folder).map_err(|e| format!("cannot read {}: {e}", folder.display()))?;
    let mut books: Vec<BookEntry> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| BOOK_EXTENSIONS.iter().any(|b| b.eq_ignore_ascii_case(x)))
        })
        .filter_map(|p| p.to_str().map(str::to_owned))
        .map(|path| {
            let row = reader.book(&path).ok().flatten();
            let stored = reader.source(&path).ok().flatten();
            entry(path, row, stored, database)
        })
        .collect();
    // By show date; books with no known date follow, by title.
    books.sort_by_cached_key(|b| (b.date.is_empty(), b.date.clone(), b.title.to_lowercase(), b.path.to_lowercase()));
    Ok(books)
}

/// One book's entry, wherever its file is: what the store knows and its BerthaML link.
pub fn describe(reader: &mut Reader, path: &str, database: &Path) -> BookEntry {
    let row = reader.book(path).ok().flatten();
    let stored = reader.source(path).ok().flatten();
    entry(path.to_owned(), row, stored, database)
}

/// The standard report for a book, from its BerthaML link, chapters, tags and audio stream.
pub fn report(reader: &mut Reader, path: &str, database: &Path) -> String {
    let row = reader.book(path).ok().flatten();
    let stored = reader.source(path).ok().flatten();
    let title = title_of(path, row.as_ref().map(|r| r.title.as_str()));
    let link = link(path, &title, stored, database);
    let evidence = linking::mka_evidence(Path::new(path), &title);
    let notes = matroska::read_source_notes(Path::new(path));
    let profile = book_profile(&notes, &title, path);
    // Earlier versions put the whole notes in the comment; the report keeps taper and equipment.
    let comment = if notes.lines().count() > 2 || notes.len() > 300 {
        crate::shows::recording_comment(&notes, &crate::catalogue::known_tapers(&profile))
    } else {
        notes
    };
    let audio = crate::shows::probe_audio(Path::new(path))
        .map(|v| {
            let s = &v["streams"][0];
            crate::report::audio_line(
                s["codec_name"].as_str().unwrap_or_default(),
                s["sample_rate"].as_str().and_then(|r| r.parse().ok()).unwrap_or(0),
                crate::shows::source_depth(s),
                s["channels"].as_u64().unwrap_or(0) as u32,
            )
        })
        .unwrap_or_default();
    let tracks = evidence
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| crate::report::ReportTrack {
            title: crate::shows::song_title(&t.title),
            seconds: t.seconds,
            set: String::new(),
            performances: link.performances.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    crate::report::render(
        &crate::report::ReportSource {
            date: evidence.date.clone(),
            place: String::new(),
            album: title,
            source_summary: profile.summary(),
            credits: profile.credits,
            comment,
            audio,
            notes: String::new(),
            tracks,
            link,
        },
        database,
    )
}

/// A song performance held by a chapter of a book in the folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnedPerformance {
    pub path: String,
    pub book_title: String,
    pub date: String,
    /// Zero-based chapter index.
    pub chapter: u32,
    pub show: String,
    pub place: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongEntry {
    pub id: String,
    pub title: String,
    /// Performances in the show database.
    pub played: u32,
    /// Performances held by books in the folder, by date.
    pub owned: Vec<OwnedPerformance>,
}

/// Every song in the show database, each with the performances the folder's books hold.
pub fn songs(reader: &mut Reader, folder: &Path, database: &Path) -> Result<Vec<SongEntry>, String> {
    let mut owned = std::collections::HashMap::<String, Vec<OwnedPerformance>>::new();
    for book in list_folder(reader, folder, database)?.into_iter().filter(|b| b.exists && !b.show_id.is_empty()) {
        let Some(show) = crate::catalogue::show(database, &book.show_id)? else { continue };
        for (chapter, held) in book.performances.iter().enumerate() {
            for performance in held {
                if let Some(entry) = show.setlist.iter().find(|e| e.id == *performance) {
                    owned.entry(entry.song_id.clone()).or_default().push(OwnedPerformance {
                        path: book.path.clone(),
                        book_title: book.title.clone(),
                        date: show.summary.date.clone(),
                        chapter: chapter as u32,
                        show: show.summary.id.clone(),
                        place: show.summary.place.clone(),
                    });
                }
            }
        }
    }
    Ok(crate::catalogue::songs(database)?
        .into_iter()
        .map(|s| {
            let mut held = owned.remove(&s.id).unwrap_or_default();
            held.sort_by(|a, b| (&a.date, &a.book_title, a.chapter).cmp(&(&b.date, &b.book_title, b.chapter)));
            held.dedup();
            SongEntry { id: s.id, title: s.title, played: s.played, owned: held }
        })
        .collect())
}

/// Books played most recently first.
pub fn recent(reader: &mut Reader, limit: usize, database: &Path) -> Result<Vec<BookEntry>, String> {
    let rows = reader.recent(limit)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let stored = reader.source(&row.path).ok().flatten();
            entry(row.path.clone(), Some(row), stored, database)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn lists_books_with_their_progress() {
        let data = tempfile::tempdir().unwrap();
        let books = tempfile::tempdir().unwrap();
        for name in ["b-second.mka", "a-first.MKA", "notes.txt"] {
            std::fs::write(books.path().join(name), b"").unwrap();
        }
        let second = books.path().join("b-second.mka").to_str().unwrap().to_owned();
        {
            let (store, _) = Store::open(data.path()).unwrap();
            let record = store.find_or_create(&second, 0).unwrap();
            store.save_position(record.id, 60_000, 120_000, false).unwrap();
            store.set_title(record.id, "Zebra").unwrap();
        }
        let mut reader = Reader::new(data.path());
        let listed = list_folder(&mut reader, books.path(), &data.path().join("no.db")).unwrap();
        let titles: Vec<&str> = listed.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, ["a-first", "Zebra"]);
        assert_eq!((listed[1].position_ms, listed[1].duration_ms), (60_000, 120_000));
        assert!(listed.iter().all(|b| b.exists));

        let recent = recent(&mut reader, 5, &data.path().join("no.db")).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "Zebra");
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        let data = tempfile::tempdir().unwrap();
        let mut reader = Reader::new(data.path());
        assert!(list_folder(&mut reader, &data.path().join("absent"), &data.path().join("no.db")).is_err());
    }
}
