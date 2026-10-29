//! The resume store: one SQLite database in WAL mode with synchronous=FULL,
//! so every commit is on disk before the call returns. The player owns the
//! only writing connection, on its own thread; the library reads through a
//! separate read-only connection.
//!
//! Schema version 2 adds the book's title, so lists can name a book without
//! opening it. Version 1 databases (Show Player 0.1 to 0.3) are upgraded
//! in place; a database from a newer version is refused and left untouched.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

pub const SCHEMA_VERSION: i32 = 2;
pub const DATABASE_FILE: &str = "bookshelf.db";

pub type StoreResult<T> = Result<T, String>;

fn text(e: rusqlite::Error) -> String {
    e.to_string()
}

pub fn now_utc() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookRecord {
    pub id: i64,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub finished: bool,
}

/// What the store knows about a book, for lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookRow {
    pub path: String,
    pub title: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub finished: bool,
    pub last_played_utc: i64,
}

pub struct Store {
    db: Option<Connection>,
}

impl Store {
    /// Opens or creates the database in `dir`. A database that fails
    /// quick_check is renamed aside and replaced with a fresh one; the
    /// returned notice then says so. A healthy database is copied to a
    /// rolling backup.
    pub fn open(dir: &Path) -> StoreResult<(Store, Option<String>)> {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = dir.join(DATABASE_FILE);
        let mut notice = None;
        let mut db = None;
        for attempt in 0..2 {
            let connection = Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(text)?;
            connection.busy_timeout(std::time::Duration::from_secs(2)).map_err(text)?;
            if healthy(&connection) {
                db = Some(connection);
                break;
            }
            drop(connection);
            if attempt == 1 {
                return Err("database is damaged and could not be replaced".into());
            }
            let aside = PathBuf::from(format!("{}.damaged-{}", path.display(), now_utc()));
            std::fs::rename(&path, &aside).map_err(|_| "database is damaged and could not be moved aside")?;
            let _ = std::fs::remove_file(format!("{}-wal", path.display()));
            let _ = std::fs::remove_file(format!("{}-shm", path.display()));
            notice = Some(format!(
                "The saved positions were damaged and have been reset. The old file was kept as {}",
                aside.display()
            ));
        }
        let db = db.expect("the loop either opened a database or returned");
        prepare_schema(&db)?;
        // Rolling backup of a database known to be healthy. After a truncating
        // checkpoint the main file is complete on its own.
        if checkpoint(&db) {
            let _ = std::fs::copy(&path, format!("{}.bak", path.display()));
        }
        Ok((Store { db: Some(db) }, notice))
    }

    fn db(&self) -> StoreResult<&Connection> {
        self.db.as_ref().ok_or_else(|| "the store is closed".to_string())
    }

    pub fn close(&mut self) {
        if let Some(db) = self.db.take() {
            checkpoint(&db);
            drop(db);
        }
    }

    /// Identity is the path (case-insensitive) until a stable file identity is read.
    pub fn find_or_create(&self, path: &str, file_size: i64) -> StoreResult<BookRecord> {
        let db = self.db()?;
        db.execute("INSERT OR IGNORE INTO book(path, file_size) VALUES(?1, ?2)", params![path, file_size])
            .map_err(text)?;
        db.query_row("SELECT id, position_ms, duration_ms, finished FROM book WHERE path = ?1", [path], |row| {
            Ok(BookRecord {
                id: row.get(0)?,
                position_ms: row.get(1)?,
                duration_ms: row.get(2)?,
                finished: row.get::<_, i64>(3)? != 0,
            })
        })
        .map_err(text)
    }

    pub fn save_position(&self, id: i64, position_ms: i64, duration_ms: i64, finished: bool) -> StoreResult<()> {
        self.db()?
            .execute(
                "UPDATE book SET position_ms = ?2, duration_ms = ?3, finished = ?4, last_played_utc = ?5,
                 missing_since_utc = NULL, save_seq = save_seq + 1 WHERE id = ?1",
                params![id, position_ms, duration_ms, i64::from(finished), now_utc()],
            )
            .map(drop)
            .map_err(text)
    }

    /// Records when a known book was last found missing. Creates nothing.
    pub fn mark_missing(&self, path: &str) -> StoreResult<()> {
        self.db()?
            .execute("UPDATE book SET missing_since_utc = ?2 WHERE path = ?1", params![path, now_utc()])
            .map(drop)
            .map_err(text)
    }

    pub fn set_title(&self, id: i64, title: &str) -> StoreResult<()> {
        self.db()?.execute("UPDATE book SET title = ?2 WHERE id = ?1", params![id, title]).map(drop).map_err(text)
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.db().ok()?.query_row("SELECT value FROM setting WHERE key = ?1", [key], |row| row.get(0)).ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> StoreResult<()> {
        self.db()?
            .execute(
                "INSERT INTO setting(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(drop)
            .map_err(text)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        self.close();
    }
}

fn healthy(db: &Connection) -> bool {
    db.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0)).is_ok_and(|r| r == "ok")
}

fn checkpoint(db: &Connection) -> bool {
    db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get::<_, i64>(0)).is_ok_and(|busy| busy == 0)
}

fn prepare_schema(db: &Connection) -> StoreResult<()> {
    let version: i32 = db.query_row("PRAGMA user_version", [], |row| row.get(0)).map_err(text)?;
    if version > SCHEMA_VERSION {
        return Err(format!(
            "the saved positions were written by a newer Show Player (schema {version}); this version reads up to {SCHEMA_VERSION}"
        ));
    }
    db.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0)).map_err(text)?;
    db.execute_batch(
        "PRAGMA synchronous=FULL;
         CREATE TABLE IF NOT EXISTS book(
             id INTEGER PRIMARY KEY,
             path TEXT NOT NULL UNIQUE COLLATE NOCASE,
             file_size INTEGER NOT NULL DEFAULT 0,
             duration_ms INTEGER NOT NULL DEFAULT 0,
             position_ms INTEGER NOT NULL DEFAULT 0,
             finished INTEGER NOT NULL DEFAULT 0,
             last_played_utc INTEGER,
             missing_since_utc INTEGER,
             save_seq INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS setting(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )
    .map_err(text)?;
    if version < 2 {
        let has_title = db.prepare("SELECT title FROM book LIMIT 0").is_ok();
        let tx = db.unchecked_transaction().map_err(text)?;
        if !has_title {
            tx.execute_batch("ALTER TABLE book ADD COLUMN title TEXT NOT NULL DEFAULT ''").map_err(text)?;
        }
        tx.execute_batch(&format!("PRAGMA user_version={SCHEMA_VERSION}")).map_err(text)?;
        tx.commit().map_err(text)?;
    }
    Ok(())
}

/// Read-only access for lists. Opened on first use, after the player has
/// created or upgraded the database.
pub struct Reader {
    path: PathBuf,
    db: Option<Connection>,
}

impl Reader {
    pub fn new(dir: &Path) -> Reader {
        Reader { path: dir.join(DATABASE_FILE), db: None }
    }

    fn db(&mut self) -> StoreResult<&Connection> {
        if self.db.is_none() {
            let db = Connection::open_with_flags(
                &self.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(text)?;
            db.busy_timeout(std::time::Duration::from_secs(2)).map_err(text)?;
            self.db = Some(db);
        }
        Ok(self.db.as_ref().expect("opened above"))
    }

    fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BookRow> {
        Ok(BookRow {
            path: row.get(0)?,
            title: row.get(1)?,
            position_ms: row.get(2)?,
            duration_ms: row.get(3)?,
            finished: row.get::<_, i64>(4)? != 0,
            last_played_utc: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        })
    }

    const COLUMNS: &str = "path, title, position_ms, duration_ms, finished, last_played_utc";

    pub fn book(&mut self, path: &str) -> StoreResult<Option<BookRow>> {
        let sql = format!("SELECT {} FROM book WHERE path = ?1", Self::COLUMNS);
        self.db()?.query_row(&sql, [path], Self::row).optional().map_err(text)
    }

    /// Books played most recently first, leaving out any never played.
    pub fn recent(&mut self, limit: usize) -> StoreResult<Vec<BookRow>> {
        let sql = format!(
            "SELECT {} FROM book WHERE last_played_utc IS NOT NULL ORDER BY last_played_utc DESC, id DESC LIMIT ?1",
            Self::COLUMNS
        );
        let db = self.db()?;
        let mut statement = db.prepare(&sql).map_err(text)?;
        let rows = statement.query_map([limit as i64], Self::row).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_saves_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let (store, notice) = Store::open(dir.path()).unwrap();
        assert!(notice.is_none());
        let record = store.find_or_create("D:\\books\\A.mka", 10).unwrap();
        assert_eq!(record.position_ms, 0);
        store.save_position(record.id, 1234, 9000, false).unwrap();
        store.set_title(record.id, "A book").unwrap();
        let again = store.find_or_create("d:\\BOOKS\\a.mka", 10).unwrap();
        assert_eq!(again.id, record.id, "paths compare without case");
        assert_eq!(again.position_ms, 1234);

        let mut reader = Reader::new(dir.path());
        let row = reader.book("D:\\books\\A.mka").unwrap().unwrap();
        assert_eq!((row.title.as_str(), row.duration_ms), ("A book", 9000));
        assert_eq!(reader.recent(10).unwrap().len(), 1);
        assert!(dir.path().join("bookshelf.db.bak").exists());
    }

    #[test]
    fn upgrades_a_version_1_database() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Connection::open(dir.path().join(DATABASE_FILE)).unwrap();
            db.execute_batch(
                "CREATE TABLE book(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE COLLATE NOCASE,
                   file_size INTEGER NOT NULL DEFAULT 0, duration_ms INTEGER NOT NULL DEFAULT 0,
                   position_ms INTEGER NOT NULL DEFAULT 0, finished INTEGER NOT NULL DEFAULT 0,
                   last_played_utc INTEGER, missing_since_utc INTEGER, save_seq INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE setting(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO book(path, position_ms, duration_ms, last_played_utc) VALUES('x.mka', 5000, 9000, 7);
                 INSERT INTO setting VALUES('volume', '55.0');
                 PRAGMA user_version=1;",
            )
            .unwrap();
        }
        let (store, _) = Store::open(dir.path()).unwrap();
        assert_eq!(store.find_or_create("x.mka", 0).unwrap().position_ms, 5000);
        assert_eq!(store.get_setting("volume").as_deref(), Some("55.0"));
        let mut reader = Reader::new(dir.path());
        assert_eq!(reader.book("x.mka").unwrap().unwrap().title, "");
    }

    #[test]
    fn refuses_a_newer_database_and_leaves_it() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Connection::open(dir.path().join(DATABASE_FILE)).unwrap();
            db.execute_batch("CREATE TABLE marker(x); PRAGMA user_version=9;").unwrap();
        }
        assert!(Store::open(dir.path()).is_err());
        let db = Connection::open(dir.path().join(DATABASE_FILE)).unwrap();
        assert_eq!(db.query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0)).unwrap(), 9);
    }

    #[test]
    fn replaces_a_damaged_database() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(DATABASE_FILE), vec![0x55u8; 8192]).unwrap();
        let (store, notice) = Store::open(dir.path()).unwrap();
        assert!(notice.unwrap().contains("damaged"));
        store.find_or_create("a.mka", 1).unwrap();
        let aside = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".damaged-"));
        assert!(aside);
    }
}
