//! SQLite plumbing shared by the repositories: the locked connection guard, opening the database, and
//! helpers for reading columns and inspecting tables.

use std::sync::MutexGuard;

use anyhow::{anyhow, Result};
use rusqlite::types::{Type, ValueRef};
use rusqlite::Connection;

/// The guard returned by `Store::lock`: the connection is `None` after `close`.
pub(crate) type Guard<'a> = MutexGuard<'a, Option<Connection>>;

/// The open connection behind a locked guard.
pub(crate) fn conn<'a>(guard: &'a Guard<'_>) -> Result<&'a Connection> {
    guard.as_ref().ok_or_else(|| anyhow!("sql: database is closed"))
}

/// Opens kiki.db with a 5 second busy timeout and WAL journaling. Foreign keys stay off: the schema
/// declares none, and the bundled SQLite would otherwise turn enforcement on by default.
pub(crate) fn open_db(log_path: &str) -> Result<Connection> {
    let db = Connection::open(log_path)?;
    db.busy_timeout(std::time::Duration::from_millis(5000))?;
    db.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))?;
    db.execute_batch("PRAGMA foreign_keys=OFF")?;
    Ok(db)
}

/// Reads a text column leniently: numbers and blobs convert to text, NULL is an error.
pub(crate) fn text(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<String> {
    match row.get_ref(idx)? {
        ValueRef::Text(t) | ValueRef::Blob(t) => Ok(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Integer(n) => Ok(n.to_string()),
        ValueRef::Real(f) => Ok(f.to_string()),
        ValueRef::Null => Err(rusqlite::Error::InvalidColumnType(
            idx,
            row.as_ref().column_name(idx)?.to_string(),
            Type::Null,
        )),
    }
}

/// The column names of a table (`PRAGMA table_info`).
pub(crate) fn table_columns(db: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = db.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    let mut names = Vec::new();
    while let Some(row) = rows.next()? {
        names.push(row.get::<_, String>(1)?);
    }
    Ok(names)
}

/// Adds each `(name, ddl)` column to `table` when it is missing.
pub(crate) fn ensure_columns(db: &Connection, table: &str, columns: &[(&str, &str)]) -> Result<()> {
    let have = table_columns(db, table)?;
    for (name, ddl) in columns {
        if !have.iter().any(|c| c == name) {
            db.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {ddl}"))?;
        }
    }
    Ok(())
}
