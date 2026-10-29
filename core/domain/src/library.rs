//! The books in a folder and the books played recently, with what the store
//! knows about each. Lists never open a book in the engine.

use std::path::Path;

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

fn entry(path: String, row: Option<BookRow>) -> BookEntry {
    let exists = Path::new(&path).is_file();
    let title = title_of(&path, row.as_ref().map(|r| r.title.as_str()));
    let (position_ms, duration_ms, finished, last_played_utc) =
        row.map_or((0, 0, false, 0), |r| (r.position_ms, r.duration_ms, r.finished, r.last_played_utc));
    let notes = matroska::read_source_notes(Path::new(&path));
    let profile = crate::catalogue::classify(&notes, &format!("{title} {path}"), "", "", "", "");
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
    }
}

/// The books directly in `folder`, by title. A store that cannot be read
/// leaves the progress fields empty rather than failing the list.
pub fn list_folder(reader: &mut Reader, folder: &Path) -> Result<Vec<BookEntry>, String> {
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
            entry(path, row)
        })
        .collect();
    books.sort_by_cached_key(|b| (b.title.to_lowercase(), b.path.to_lowercase()));
    Ok(books)
}

/// Books played most recently first.
pub fn recent(reader: &mut Reader, limit: usize) -> Result<Vec<BookEntry>, String> {
    Ok(reader.recent(limit)?.into_iter().map(|row| entry(row.path.clone(), Some(row))).collect())
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
        let listed = list_folder(&mut reader, books.path()).unwrap();
        let titles: Vec<&str> = listed.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, ["a-first", "Zebra"]);
        assert_eq!((listed[1].position_ms, listed[1].duration_ms), (60_000, 120_000));
        assert!(listed.iter().all(|b| b.exists));

        let recent = recent(&mut reader, 5).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "Zebra");
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        let data = tempfile::tempdir().unwrap();
        let mut reader = Reader::new(data.path());
        assert!(list_folder(&mut reader, &data.path().join("absent")).is_err());
    }
}
