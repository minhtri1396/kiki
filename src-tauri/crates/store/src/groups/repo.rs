//! Pattern: Repository. `GroupRepo` is the only place that runs SQL against `groups`.

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};

use super::{ErrGroupNotFound, Group, GroupInput};
use crate::db::{ensure_columns, text};
use crate::store::now;
use crate::tasks::{epic_key_matches, new_task_id};

const GROUP_SELECT: &str =
    "SELECT id, name, description, epic_key, epic_board_id, epic_board_name, created_at, deleted_at FROM groups";

pub(crate) struct GroupRepo<'a> {
    db: &'a Connection,
}

impl<'a> GroupRepo<'a> {
    pub(crate) fn new(db: &'a Connection) -> GroupRepo<'a> {
        GroupRepo { db }
    }

    pub(crate) fn init(db: &Connection) -> Result<()> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS groups (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			epic_key TEXT NOT NULL,
			epic_board_id TEXT NOT NULL,
			epic_board_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			deleted_at TEXT NOT NULL DEFAULT ''
		)",
        )?;
        ensure_columns(db, "groups", &[("deleted_at", "deleted_at TEXT NOT NULL DEFAULT ''")])
    }

    /// Every group, newest first.
    pub(crate) fn list(&self) -> Result<Vec<Group>> {
        let mut stmt = self
            .db
            .prepare(&format!("{GROUP_SELECT} ORDER BY created_at DESC, id DESC"))?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(scan_group(row)?);
        }
        Ok(out)
    }

    pub(crate) fn get(&self, id: &str) -> Result<Group> {
        let found = self
            .db
            .query_row(&format!("{GROUP_SELECT} WHERE id = ?"), params![id.trim()], scan_group)
            .optional()?;
        found.ok_or_else(|| ErrGroupNotFound.into())
    }

    /// Stores a group as given; the caller validated it.
    pub(crate) fn create(&self, input: &GroupInput) -> Result<Group> {
        let id = new_task_id()?;
        let created = now();
        self.db.execute(
            "INSERT INTO groups (id, name, description, epic_key, epic_board_id, epic_board_name, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![id, input.name, input.description, input.epic_key, input.epic_board_id, input.epic_board_name, created],
        )?;
        self.get(&id)
    }

    /// Marks a group deleted. Deleting a deleted group is fine; an unknown id is not found.
    pub(crate) fn mark_deleted(&self, id: &str) -> Result<()> {
        let n = self.db.execute(
            "UPDATE groups SET deleted_at = ? WHERE id = ? AND deleted_at = ''",
            params![now(), id],
        )?;
        if n > 0 {
            return Ok(());
        }
        if self.get(id)?.deleted {
            return Ok(());
        }
        Err(ErrGroupNotFound.into())
    }

    /// Turns tasks that already have a Jira epic into groups, then points those tasks at the group.
    pub(crate) fn migrate_task_groups(&self) -> Result<()> {
        for key in self.legacy_epic_keys()? {
            if !epic_key_matches(&key.to_uppercase()) {
                continue;
            }
            let Some(group_id) = self.group_for_epic(&key)? else {
                continue;
            };
            self.db.execute(
                "UPDATE tasks SET group_id = ?, parent_id = '', epic_owner = 0 WHERE epic_key = ? AND group_id = ''",
                params![group_id, key],
            )?;
        }
        Ok(())
    }

    /// The epic keys of tasks that are not in a group yet.
    fn legacy_epic_keys(&self) -> Result<Vec<String>> {
        let mut keys: Vec<String> = Vec::new();
        let mut stmt = self
            .db
            .prepare("SELECT DISTINCT epic_key FROM tasks WHERE group_id = '' AND epic_key != ''")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            keys.push(text(row, 0)?.trim().to_string());
        }
        Ok(keys)
    }

    /// The group for an epic key: the existing one, or a new one named after the task that owns the epic.
    /// `None` when the epic has no board to build a group from.
    fn group_for_epic(&self, key: &str) -> Result<Option<String>> {
        let existing: Option<String> = self
            .db
            .query_row(
                "SELECT id FROM groups WHERE epic_key = ?",
                params![key.to_uppercase()],
                |row| text(row, 0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(Some(id));
        }
        let Some((mut name, description, board_id, board_name)) = self.epic_source(key)? else {
            return Err(anyhow!("sql: no rows in result set"));
        };
        if board_id.trim().is_empty() {
            return Ok(None);
        }
        if name.trim().is_empty() {
            name = key.to_uppercase();
        }
        let group = self.create(&GroupInput {
            name,
            description,
            epic_key: key.to_string(),
            epic_board_id: board_id,
            epic_board_name: board_name,
        })?;
        Ok(Some(group.id))
    }

    /// `(name, description, board id, board name)` of the oldest task that owns the epic, else the oldest task using it.
    fn epic_source(&self, key: &str) -> Result<Option<(String, String, String, String)>> {
        let pick = |sql: &str| -> Result<Option<(String, String, String, String)>> {
            Ok(self
                .db
                .query_row(sql, params![key], |row| {
                    Ok((text(row, 0)?, text(row, 1)?, text(row, 2)?, text(row, 3)?))
                })
                .optional()?)
        };
        match pick("SELECT name, description, epic_board_id, epic_board_name FROM tasks WHERE epic_key = ? AND epic_owner = 1 ORDER BY created_at LIMIT 1")? {
            Some(found) => Ok(Some(found)),
            None => pick("SELECT name, description, epic_board_id, epic_board_name FROM tasks WHERE epic_key = ? ORDER BY created_at LIMIT 1"),
        }
    }
}

fn scan_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<Group> {
    let deleted_at = text(row, 7)?;
    Ok(Group {
        id: text(row, 0)?,
        name: text(row, 1)?,
        description: text(row, 2)?,
        epic_key: text(row, 3)?,
        epic_board_id: text(row, 4)?,
        epic_board_name: text(row, 5)?,
        created_at: text(row, 6)?,
        deleted: !deleted_at.trim().is_empty(),
    })
}
