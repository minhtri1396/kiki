//! Pattern: Repository. `RunRepo` is the only place that runs SQL against `task_runs`.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{params, Connection};

use super::TaskRun;
use crate::db::{ensure_columns, text};
use crate::tasks::{ErrTaskNotFound, Task};

const RUN_SELECT: &str = "SELECT task_id, agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at FROM task_runs";

pub(crate) struct RunRepo<'a> {
    db: &'a Connection,
}

impl<'a> RunRepo<'a> {
    pub(crate) fn new(db: &'a Connection) -> RunRepo<'a> {
        RunRepo { db }
    }

    pub(crate) fn init(db: &Connection) -> Result<()> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_runs (
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
        )?;
        ensure_columns(
            db,
            "task_runs",
            &[
                ("ask_prompt", "ask_prompt TEXT NOT NULL DEFAULT ''"),
                ("ask_options", "ask_options TEXT NOT NULL DEFAULT '[]'"),
            ],
        )
    }

    /// Records a fresh terminal for one agent; a new start replaces the previous terminal, log and question.
    pub(crate) fn start(&self, task_id: &str, agent_id: &str, terminal: &str, stamp: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO task_runs (task_id, agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at)
		VALUES (?, ?, ?, 'running', '', '', '[]', ?, ?)
		ON CONFLICT(task_id, agent_id) DO UPDATE SET
			terminal = excluded.terminal,
			orca_status = 'running',
			log = '',
			ask_prompt = '',
			ask_options = '[]',
			started_at = excluded.started_at,
			updated_at = excluded.updated_at",
            params![task_id, agent_id, terminal, stamp, stamp],
        )?;
        Ok(())
    }

    /// Stores the latest Orca status and transcript. A run that does not exist is not found.
    pub(crate) fn update_log(&self, task_id: &str, agent_id: &str, status: &str, log: &str, stamp: &str) -> Result<()> {
        let n = self.db.execute(
            "UPDATE task_runs SET orca_status = ?, log = ?, updated_at = ? WHERE task_id = ? AND agent_id = ?",
            params![status, tail_text(log, 8000), stamp, task_id, agent_id],
        )?;
        if n == 0 {
            return Err(ErrTaskNotFound.into());
        }
        Ok(())
    }

    /// Stores the question the agent is waiting on. A questions block replaces the single question.
    pub(crate) fn set_question(
        &self,
        task_id: &str,
        agent_id: &str,
        prompt: &str,
        options: &[String],
        questions: &[super::AskQuestion],
    ) -> Result<()> {
        let (prompt, encoded) = if questions.is_empty() {
            (prompt.trim(), serde_json::to_string(options)?)
        } else {
            ("", serde_json::to_string(questions)?)
        };
        self.db.execute(
            "UPDATE task_runs SET ask_prompt = ?, ask_options = ? WHERE task_id = ? AND agent_id = ?",
            params![prompt, encoded, task_id, agent_id],
        )?;
        Ok(())
    }

    /// Marks the terminal running again and clears the question, after the user replied.
    pub(crate) fn mark_running(&self, task_id: &str, agent_id: &str, stamp: &str) -> Result<()> {
        self.db.execute(
            "UPDATE task_runs SET orca_status = 'running', ask_prompt = '', ask_options = '[]', updated_at = ? WHERE task_id = ? AND agent_id = ?",
            params![stamp, task_id, agent_id],
        )?;
        Ok(())
    }

    /// Terminals that have not exited, oldest task first.
    pub(crate) fn open(&self) -> Result<Vec<TaskRun>> {
        let mut stmt = self.db.prepare(&format!(
            "{RUN_SELECT} WHERE orca_status != 'exited' ORDER BY task_id, agent_id"
        ))?;
        scan_runs(&mut stmt)
    }

    /// Fills `Task::runs` for every task.
    pub(crate) fn attach_to(&self, tasks: &mut [Task]) -> Result<()> {
        for task in tasks.iter_mut() {
            task.runs = Vec::new();
        }
        if tasks.is_empty() {
            return Ok(());
        }
        let mut stmt = self.db.prepare(&format!("{RUN_SELECT} ORDER BY task_id, agent_id"))?;
        let mut by_task: HashMap<String, Vec<TaskRun>> = HashMap::new();
        for run in scan_runs(&mut stmt)? {
            by_task.entry(run.task_id.clone()).or_default().push(run);
        }
        for task in tasks.iter_mut() {
            if let Some(list) = by_task.get(&task.id) {
                task.runs = list.clone();
            }
        }
        Ok(())
    }
}

fn scan_runs(stmt: &mut rusqlite::Statement<'_>) -> Result<Vec<TaskRun>> {
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let options = text(row, 6)?;
        let (ask_options, ask_questions) = decode_ask(&options);
        out.push(TaskRun {
            task_id: text(row, 0)?,
            agent_id: text(row, 1)?,
            terminal: text(row, 2)?,
            status: text(row, 3)?,
            log: text(row, 4)?,
            ask_prompt: text(row, 5)?,
            ask_options,
            ask_questions,
            started_at: text(row, 7)?,
            updated_at: text(row, 8)?,
        });
    }
    Ok(out)
}

/// A questions block is stored as objects. A single question is stored as a list of option strings.
fn decode_ask(raw: &str) -> (Vec<String>, Vec<super::AskQuestion>) {
    let raw = raw.trim();
    if raw.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if let Ok(questions) = serde_json::from_str::<Vec<super::AskQuestion>>(raw) {
        if questions.iter().any(|question| !question.prompt.trim().is_empty()) {
            return (Vec::new(), questions);
        }
    }
    (decode_options(raw), Vec::new())
}

fn decode_options(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Option<Vec<String>>>(raw)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// The last `max` characters of `text`.
fn tail_text(text: &str, max: usize) -> String {
    let runes: Vec<char> = text.chars().collect();
    if runes.len() <= max {
        return text.to_string();
    }
    runes[runes.len() - max..].iter().collect()
}
