//! The `tasks` and `task_agents` tables: creating them, adding columns older databases lack, and moving
//! integer task ids to text.

use anyhow::Result;
use rusqlite::Connection;

use crate::db::{ensure_columns, text};

pub(crate) fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS tasks (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			column_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			done_at TEXT NOT NULL,
				branch_slug TEXT NOT NULL DEFAULT '',
					branch_for TEXT NOT NULL DEFAULT '',
					started INTEGER NOT NULL DEFAULT 0,
					parent_id TEXT NOT NULL DEFAULT '',
					epic_key TEXT NOT NULL DEFAULT '',
					epic_board_id TEXT NOT NULL DEFAULT '',
					epic_board_name TEXT NOT NULL DEFAULT '',
					epic_owner INTEGER NOT NULL DEFAULT 0,
					group_id TEXT NOT NULL DEFAULT ''
				)",
    )?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS task_agents (
			task_id TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			status TEXT NOT NULL,
			position INTEGER NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		)",
    )?;
    ensure_task_columns(db)
}

fn ensure_task_columns(db: &Connection) -> Result<()> {
    ensure_columns(
        db,
        "tasks",
        &[
            ("branch_slug", "branch_slug TEXT NOT NULL DEFAULT ''"),
            ("branch_for", "branch_for TEXT NOT NULL DEFAULT ''"),
            ("started", "started INTEGER NOT NULL DEFAULT 0"),
            ("parent_id", "parent_id TEXT NOT NULL DEFAULT ''"),
            ("epic_key", "epic_key TEXT NOT NULL DEFAULT ''"),
            ("epic_board_id", "epic_board_id TEXT NOT NULL DEFAULT ''"),
            ("epic_board_name", "epic_board_name TEXT NOT NULL DEFAULT ''"),
            ("epic_owner", "epic_owner INTEGER NOT NULL DEFAULT 0"),
            ("group_id", "group_id TEXT NOT NULL DEFAULT ''"),
        ],
    )
}

/// Copies integer task ids into text. Existing ids stay their
/// decimal form so an Orca group named with that id still matches. New tasks
/// use ULIDs.
pub(crate) fn migrate_ids(db: &Connection) -> Result<()> {
    let kind: String = db.query_row(
        "SELECT type FROM pragma_table_info('tasks') WHERE name = 'id'",
        [],
        |row| text(row, 0),
    )?;
    if !kind.eq_ignore_ascii_case("INTEGER") {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    let statements = [
        "CREATE TABLE tasks_text (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			column_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			done_at TEXT NOT NULL,
			branch_slug TEXT NOT NULL DEFAULT '',
			branch_for TEXT NOT NULL DEFAULT '',
			started INTEGER NOT NULL DEFAULT 0,
			parent_id TEXT NOT NULL DEFAULT '',
			epic_key TEXT NOT NULL DEFAULT '',
			epic_board_id TEXT NOT NULL DEFAULT '',
			epic_board_name TEXT NOT NULL DEFAULT '',
			epic_owner INTEGER NOT NULL DEFAULT 0,
			group_id TEXT NOT NULL DEFAULT ''
		)",
        "INSERT INTO tasks_text (id, name, description, column_name, created_at, updated_at, done_at, branch_slug, branch_for, started, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id)
			SELECT CAST(id AS TEXT), name, description, column_name, created_at, updated_at, done_at, branch_slug, branch_for, started, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id FROM tasks",
        "DROP TABLE tasks",
        "ALTER TABLE tasks_text RENAME TO tasks",
        "CREATE TABLE task_agents_text (
			task_id TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			status TEXT NOT NULL,
			position INTEGER NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		)",
        "INSERT INTO task_agents_text (task_id, agent_id, status, position)
			SELECT CAST(task_id AS TEXT), agent_id, status, position FROM task_agents",
        "DROP TABLE task_agents",
        "ALTER TABLE task_agents_text RENAME TO task_agents",
        "CREATE TABLE task_runs_text (
			task_id TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			terminal TEXT NOT NULL,
			orca_status TEXT NOT NULL,
			log TEXT NOT NULL,
			ask_prompt TEXT NOT NULL DEFAULT '',
			ask_options TEXT NOT NULL DEFAULT '[]',
			started_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		)",
        "INSERT INTO task_runs_text (task_id, agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at)
			SELECT CAST(task_id AS TEXT), agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at FROM task_runs",
        "DROP TABLE task_runs",
        "ALTER TABLE task_runs_text RENAME TO task_runs",
    ];
    for statement in statements {
        tx.execute_batch(statement)?;
    }
    tx.commit()?;
    Ok(())
}
