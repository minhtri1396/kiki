//! Pattern: Repository. `TaskRepo` is the only place that runs SQL against `tasks` and `task_agents`.
//! It takes a connection the caller already holds the store lock for.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use super::ids::new_task_id;
use super::model::{ErrTaskNotFound, Task, TaskAgent, TaskDraft};
use super::state::{AgentStatus, Column};
use crate::db::text;
use crate::runs::RunRepo;
use crate::store::now;

const TASK_SELECT: &str = "id, name, description, column_name, created_at, updated_at, done_at, branch_slug, branch_for, started, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id";

pub(crate) struct TaskRepo<'a> {
    db: &'a Connection,
}

impl<'a> TaskRepo<'a> {
    pub(crate) fn new(db: &'a Connection) -> TaskRepo<'a> {
        TaskRepo { db }
    }

    /// Every task with its agents and runs, newest activity first.
    pub(crate) fn list(&self) -> Result<Vec<Task>> {
        let mut out: Vec<Task> = Vec::new();
        {
            let mut stmt = self.db.prepare(&format!(
                "SELECT {TASK_SELECT} FROM tasks ORDER BY updated_at DESC, id DESC"
            ))?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                out.push(scan_task(row)?);
            }
        }
        let mut by_task: HashMap<String, Vec<TaskAgent>> = HashMap::new();
        {
            let mut stmt = self
                .db
                .prepare("SELECT task_id, agent_id, status FROM task_agents ORDER BY task_id, position, agent_id")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let task_id = text(row, 0)?;
                by_task.entry(task_id).or_default().push(TaskAgent {
                    agent_id: text(row, 1)?,
                    status: AgentStatus::from_stored(&text(row, 2)?),
                });
            }
        }
        for task in out.iter_mut() {
            task.agents = by_task.remove(&task.id).unwrap_or_default();
        }
        RunRepo::new(self.db).attach_to(&mut out)?;
        Ok(out)
    }

    /// One task with its agents and runs.
    pub(crate) fn get(&self, id: &str) -> Result<Task> {
        if id.is_empty() {
            return Err(ErrTaskNotFound.into());
        }
        let found = self
            .db
            .query_row(
                &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?"),
                params![id],
                scan_task,
            )
            .optional()?;
        let Some(mut item) = found else {
            return Err(ErrTaskNotFound.into());
        };
        item.agents = self.agents(id)?;
        let mut tasks = vec![item];
        RunRepo::new(self.db).attach_to(&mut tasks)?;
        Ok(tasks.remove(0))
    }

    /// The agents of one task in the order the user picked them.
    fn agents(&self, task_id: &str) -> Result<Vec<TaskAgent>> {
        let mut stmt = self
            .db
            .prepare("SELECT agent_id, status FROM task_agents WHERE task_id = ? ORDER BY position, agent_id")?;
        let mut rows = stmt.query(params![task_id])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(TaskAgent {
                agent_id: text(row, 0)?,
                status: AgentStatus::from_stored(&text(row, 1)?),
            });
        }
        Ok(out)
    }

    /// Stores a new task created at `stamp`.
    pub(crate) fn insert(&self, draft: &TaskDraft, stamp: &str) -> Result<Task> {
        let tx = self.db.unchecked_transaction()?;
        let id = new_task_id()?;
        tx.execute(
            "INSERT INTO tasks (id, name, description, column_name, created_at, updated_at, done_at, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id)
				VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id,
                draft.name,
                draft.description,
                draft.column.as_str(),
                stamp,
                stamp,
                "",
                draft.parent_id,
                draft.epic_key,
                draft.epic_board_id,
                draft.epic_board_name,
                i64::from(draft.epic_owner),
                draft.group_id
            ],
        )?;
        replace_agents(&tx, &id, &draft.agents)?;
        tx.commit()?;
        self.get(&id)
    }

    /// Replaces the editable fields of task `id`. `created_at` is never touched.
    pub(crate) fn update(&self, id: &str, draft: &TaskDraft, updated_at: &str, done_at: &str) -> Result<Task> {
        let tx = self.db.unchecked_transaction()?;
        let n = tx.execute(
            "UPDATE tasks SET name = ?, description = ?, column_name = ?, updated_at = ?, done_at = ?, parent_id = ?, epic_key = ?, epic_board_id = ?, epic_board_name = ?, epic_owner = ?, group_id = ? WHERE id = ?",
            params![
                draft.name,
                draft.description,
                draft.column.as_str(),
                updated_at,
                done_at,
                draft.parent_id,
                draft.epic_key,
                draft.epic_board_id,
                draft.epic_board_name,
                i64::from(draft.epic_owner),
                draft.group_id,
                id
            ],
        )?;
        if n == 0 {
            return Err(ErrTaskNotFound.into());
        }
        replace_agents(&tx, id, &draft.agents)?;
        tx.commit()?;
        self.get(id)
    }

    /// Deletes a task with its agents and runs; children are detached from it.
    pub(crate) fn delete(&self, id: &str) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute("UPDATE tasks SET parent_id = '' WHERE parent_id = ?", params![id])?;
        tx.execute("DELETE FROM task_agents WHERE task_id = ?", params![id])?;
        tx.execute("DELETE FROM task_runs WHERE task_id = ?", params![id])?;
        let n = tx.execute("DELETE FROM tasks WHERE id = ?", params![id])?;
        if n == 0 {
            // Dropping the transaction rolls it back.
            return Err(ErrTaskNotFound.into());
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn set_branch(&self, id: &str, slug: &str, source: &str) -> Result<()> {
        let n = self.db.execute(
            "UPDATE tasks SET branch_slug = ?, branch_for = ? WHERE id = ?",
            params![slug, source, id],
        )?;
        if n == 0 {
            return Err(ErrTaskNotFound.into());
        }
        Ok(())
    }

    pub(crate) fn exists(&self, id: &str) -> Result<bool> {
        let n: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM tasks WHERE id = ?", params![id], |row| row.get(0))?;
        Ok(n > 0)
    }

    pub(crate) fn mark_started(&self, task_id: &str) -> Result<()> {
        self.db
            .execute("UPDATE tasks SET started = 1 WHERE id = ?", params![task_id])?;
        Ok(())
    }

    /// One agent's status on a task, or `None` when the agent is not on it.
    pub(crate) fn agent_status(&self, task_id: &str, agent_id: &str) -> Result<Option<AgentStatus>> {
        let stored: Option<String> = self
            .db
            .query_row(
                "SELECT status FROM task_agents WHERE task_id = ? AND agent_id = ?",
                params![task_id, agent_id],
                |row| text(row, 0),
            )
            .optional()?;
        Ok(stored.map(|text| AgentStatus::from_stored(&text)))
    }

    pub(crate) fn set_agent_status(&self, task_id: &str, agent_id: &str, status: AgentStatus) -> Result<()> {
        self.db.execute(
            "UPDATE task_agents SET status = ? WHERE task_id = ? AND agent_id = ?",
            params![status.as_str(), task_id, agent_id],
        )?;
        Ok(())
    }

    /// Moves a started task to the column its agents' statuses call for.
    pub(crate) fn sync_column(&self, task_id: &str) -> Result<()> {
        let (started, column): (i64, String) = self.db.query_row(
            "SELECT started, column_name FROM tasks WHERE id = ?",
            params![task_id],
            |row| Ok((row.get(0)?, text(row, 1)?)),
        )?;
        if started == 0 {
            return Ok(());
        }
        let current = Column::from_stored(&column);
        let statuses: Vec<AgentStatus> = self.agents(task_id)?.into_iter().map(|agent| agent.status).collect();
        let next = Column::from_agents(current, true, &statuses);
        if next == current {
            return Ok(());
        }
        let done_at = if next.is_done() { now() } else { String::new() };
        self.db.execute(
            "UPDATE tasks SET column_name = ?, done_at = ?, updated_at = ? WHERE id = ?",
            params![next.as_str(), done_at, now(), task_id],
        )?;
        Ok(())
    }
}

/// Rewrites a task's agent rows. An agent already on the task keeps its stored status; a new one is pending.
/// Runs of agents that left the task are removed.
fn replace_agents(tx: &Connection, task_id: &str, agents: &[String]) -> Result<()> {
    let mut previous: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = tx.prepare("SELECT agent_id, status FROM task_agents WHERE task_id = ?")?;
        let mut rows = stmt.query(params![task_id])?;
        while let Some(row) = rows.next()? {
            previous.insert(text(row, 0)?, text(row, 1)?);
        }
    }
    tx.execute("DELETE FROM task_agents WHERE task_id = ?", params![task_id])?;
    if !agents.is_empty() {
        let mut stmt =
            tx.prepare("INSERT INTO task_agents (task_id, agent_id, status, position) VALUES (?, ?, ?, ?)")?;
        for (i, agent_id) in agents.iter().enumerate() {
            let status = match previous.get(agent_id) {
                Some(stored) if !stored.is_empty() => stored.clone(),
                _ => AgentStatus::Pending.as_str().to_string(),
            };
            stmt.execute(params![task_id, agent_id, status, i as i64])?;
        }
    }
    tx.execute(
        "DELETE FROM task_runs WHERE task_id = ? AND agent_id NOT IN (SELECT agent_id FROM task_agents WHERE task_id = ?)",
        params![task_id, task_id],
    )?;
    Ok(())
}

fn scan_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let started: i64 = row.get(9)?;
    let owner: i64 = row.get(14)?;
    Ok(Task {
        id: text(row, 0)?,
        name: text(row, 1)?,
        description: text(row, 2)?,
        column: Column::from_stored(&text(row, 3)?),
        created_at: text(row, 4)?,
        updated_at: text(row, 5)?,
        done_at: text(row, 6)?,
        branch_slug: text(row, 7)?,
        branch_for: text(row, 8)?,
        started: started != 0,
        parent_id: text(row, 10)?,
        epic_key: text(row, 11)?,
        epic_board_id: text(row, 12)?,
        epic_board_name: text(row, 13)?,
        epic_owner: owner != 0,
        group_id: text(row, 15)?,
        agents: Vec::new(),
        runs: Vec::new(),
    })
}
