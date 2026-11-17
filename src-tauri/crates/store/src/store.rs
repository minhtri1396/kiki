//! Pattern: Facade. [`Store`] is the one object the rest of the app talks to. It owns the setup file path and
//! the SQLite connection and keeps every public method name stable, while the work lives in repositories:
//! `setup` (kiki.yaml, repos, tools, MCP files), `tasks`, `groups`, `runs` and `launches`.

use std::fs;
use std::sync::Mutex;

use anyhow::Result;
use kiki_util::path::{dir, join};
use rusqlite::Connection;

use crate::db::{conn, open_db, Guard};
use crate::fsutil;
use crate::schema;
use crate::setup::{default_doc, read_doc, write_doc};

/// Store is the console setup file and the run log.
///
/// It is shared across threads (`Arc<Store>`): every method takes `&self` and the SQLite
/// connection sits behind the same mutex that serialises the setup-file edits.
pub struct Store {
    pub(crate) path: String,
    pub(crate) log_path: String,
    pub(crate) db: Mutex<Option<Connection>>,
}

/// Open opens or creates the setup file.
pub fn open(path: &str) -> Result<Store> {
    Store::open(path)
}

impl Store {
    /// Open opens or creates the setup file and the run log next to it.
    pub fn open(path: &str) -> Result<Store> {
        fsutil::mkdir_all(&dir(path), 0o755)?;
        let s = Store {
            path: path.to_string(),
            log_path: join(&[&dir(path), "kiki.db"]),
            db: Mutex::new(None),
        };
        match fs::metadata(path) {
            Ok(_) => {}
            Err(err) => {
                if !fsutil::not_exist(&err) {
                    return Err(err.into());
                }
                write_doc(&s.path, &default_doc())?;
            }
        }
        read_doc(&s.path)?;
        let db = open_db(&s.log_path)?;
        // On any error below `db` is dropped, which closes it.
        schema::init(&db, &s.path)?;
        *s.db.lock().unwrap_or_else(|e| e.into_inner()) = Some(db);
        Ok(s)
    }

    pub(crate) fn lock(&self) -> Guard<'_> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `f` with the open connection while holding the store lock.
    pub(crate) fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.lock();
        f(conn(&guard)?)
    }

    /// Close releases the run log.
    pub fn close(&self) -> Result<()> {
        let mut guard = self.lock();
        match guard.take() {
            None => Ok(()),
            Some(db) => db.close().map_err(|(_, err)| err.into()),
        }
    }

    /// File returns the setup file path.
    pub fn file(&self) -> &str {
        &self.path
    }

    /// LogFile returns the SQLite file that stores recent runs, next to the setup file.
    pub fn log_file(&self) -> &str {
        &self.log_path
    }
}

/// DataDir is the .kiki folder next to the app. KIKI stores the files it creates there.
pub fn data_dir(dir: &str) -> String {
    join(&[dir, ".kiki"])
}

/// Path is kiki.yaml inside the .kiki folder.
pub fn path(dir: &str) -> String {
    join(&[&data_dir(dir), "kiki.yaml"])
}

pub(crate) fn now() -> String {
    kiki_util::now_rfc3339()
}

/// DetailLimit keeps launch details small enough for the UI.
pub fn detail_limit(text: &str) -> String {
    const MAX: usize = 8000;
    if text.len() <= MAX {
        return text.to_string();
    }
    // A cut inside a character becomes U+FFFD, as when the text is later encoded as JSON.
    let head = String::from_utf8_lossy(&text.as_bytes()[..MAX]).into_owned();
    format!("{head}\n… ({} bytes truncated)", text.len() - MAX)
}
