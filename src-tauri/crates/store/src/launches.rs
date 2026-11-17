//! The run log: agent starts recorded in SQLite. `Store` methods (facade) over [`LaunchRepo`].

use std::fmt;
use std::fs;

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::text;
use crate::setup::lenient::{get_i64, get_str, read_map, rows};
use crate::setup::{read_doc, write_doc};
use crate::store::{now, Store};

/// ErrNotFound is returned when a run id is not in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrNotFound;

impl fmt::Display for ErrNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("run was not found")
    }
}

impl std::error::Error for ErrNotFound {}

/// Reports whether `err` is [`ErrNotFound`].
pub fn is_not_found(err: &anyhow::Error) -> bool {
    err.downcast_ref::<ErrNotFound>().is_some()
}

/// Launch is one recorded agent start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Launch {
    #[serde(rename = "id")]
    pub id: i64,
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "runtime")]
    pub runtime: String,
    #[serde(rename = "note")]
    pub note: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "detail")]
    pub detail: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// Pattern: Repository. The only place that runs SQL against `launches`.
pub(crate) struct LaunchRepo<'a> {
    db: &'a Connection,
}

impl<'a> LaunchRepo<'a> {
    pub(crate) fn new(db: &'a Connection) -> LaunchRepo<'a> {
        LaunchRepo { db }
    }

    pub(crate) fn init(db: &Connection) -> Result<()> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS launches (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		agent_id TEXT NOT NULL,
		runtime TEXT NOT NULL,
		note TEXT NOT NULL,
		status TEXT NOT NULL,
		detail TEXT NOT NULL,
		created_at TEXT NOT NULL
	)",
        )?;
        Ok(())
    }

    /// Copies runs still stored in kiki.yaml into SQLite, then removes that list from the setup file.
    /// Agents read the setup file and do not need the run log.
    pub(crate) fn migrate_from_setup(db: &Connection, setup_path: &str) -> Result<()> {
        let raw = fs::read(setup_path)?;
        let text = String::from_utf8_lossy(&raw).into_owned();
        let launches = read_launches(&text).map_err(|err| anyhow!("read setup file: {err}"))?;
        if launches.is_empty() {
            return Ok(());
        }
        LaunchRepo::new(db).import(&launches)?;
        let setup = read_doc(setup_path)?;
        write_doc(setup_path, &setup)
    }

    /// Inserts launches that keep their ids, skipping ones already stored and ones without a positive id.
    fn import(&self, launches: &[Launch]) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO launches
		(id, agent_id, runtime, note, status, detail, created_at)
		VALUES (?, ?, ?, ?, ?, ?, ?)",
            )?;
            for item in launches.iter().filter(|item| item.id > 0) {
                stmt.execute(params![
                    item.id,
                    item.agent_id,
                    item.runtime,
                    item.note,
                    item.status,
                    item.detail,
                    item.created_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn add(&self, item: &mut Launch) -> Result<()> {
        self.db.execute(
            "INSERT INTO launches (agent_id, runtime, note, status, detail, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                item.agent_id,
                item.runtime,
                item.note,
                item.status,
                item.detail,
                item.created_at
            ],
        )?;
        item.id = self.db.last_insert_rowid();
        Ok(())
    }

    fn newest(&self, limit: i64) -> Result<Vec<Launch>> {
        let mut stmt = self.db.prepare(
            "SELECT id, agent_id, runtime, note, status, detail, created_at
		FROM launches ORDER BY id DESC LIMIT ?",
        )?;
        let mut rows = stmt.query(params![limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(Launch {
                id: row.get(0)?,
                agent_id: text(row, 1)?,
                runtime: text(row, 2)?,
                note: text(row, 3)?,
                status: text(row, 4)?,
                detail: text(row, 5)?,
                created_at: text(row, 6)?,
            });
        }
        Ok(out)
    }

    fn delete(&self, id: i64) -> Result<()> {
        let n = self.db.execute("DELETE FROM launches WHERE id = ?", params![id])?;
        if n == 0 {
            return Err(ErrNotFound.into());
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        self.db.execute("DELETE FROM launches", [])?;
        Ok(())
    }
}

/// Decodes the `launches` list of an older setup file.
fn read_launches(raw: &str) -> Result<Vec<Launch>> {
    let map = read_map(raw)?;
    Ok(rows(&map, "launches", "Launch")?
        .iter()
        .map(|row| Launch {
            id: get_i64(row, "id"),
            agent_id: get_str(row, "agent_id"),
            runtime: get_str(row, "runtime"),
            note: get_str(row, "note"),
            status: get_str(row, "status"),
            detail: get_str(row, "detail"),
            created_at: get_str(row, "created_at"),
        })
        .collect())
}

impl Store {
    /// AddLaunch stores the result of an agent start.
    pub fn add_launch(&self, agent_id: &str, runtime: &str, note: &str, status: &str, detail: &str) -> Result<Launch> {
        let mut item = Launch {
            id: 0,
            agent_id: agent_id.to_string(),
            runtime: runtime.to_string(),
            note: note.to_string(),
            status: status.to_string(),
            detail: detail.to_string(),
            created_at: now(),
        };
        self.with_db(|db| LaunchRepo::new(db).add(&mut item))?;
        Ok(item)
    }

    /// Launches returns the newest launches.
    pub fn launches(&self, limit: i64) -> Result<Vec<Launch>> {
        let limit = if limit <= 0 { 15 } else { limit };
        self.with_db(|db| LaunchRepo::new(db).newest(limit))
    }

    /// DeleteLaunch removes one run.
    pub fn delete_launch(&self, id: i64) -> Result<()> {
        self.with_db(|db| LaunchRepo::new(db).delete(id))
    }

    /// ClearLaunches removes every run.
    pub fn clear_launches(&self) -> Result<()> {
        self.with_db(|db| LaunchRepo::new(db).clear())
    }
}
