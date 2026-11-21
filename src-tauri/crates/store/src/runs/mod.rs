//! Orca terminals for a task, one per agent. `Store` methods (facade) over [`repo::RunRepo`] and
//! [`TaskRepo`], with the status changes decided by the [`AgentStatus`] state machine.

mod repo;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub(crate) use repo::RunRepo;

use crate::store::{now, Store};
use crate::tasks::{AgentStatus, ErrTaskNotFound, RunPhase, Task, TaskError, TaskRepo};

/// One question the agent sent in a questions block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AskQuestion {
    pub prompt: String,
    pub detail: String,
    pub options: Vec<String>,
    /// An answer the agent suggests. Empty when it has none.
    pub suggestion: String,
}

/// TaskRun is one agent's Orca terminal for a task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskRun {
    #[serde(skip)]
    pub task_id: String,
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub terminal: String,
    pub status: String,
    pub log: String,
    #[serde(rename = "askPrompt")]
    pub ask_prompt: String,
    #[serde(rename = "askOptions")]
    pub ask_options: Vec<String>,
    /// Questions from a questions block. Empty `options` means the user types an answer.
    #[serde(rename = "askQuestions")]
    pub ask_questions: Vec<AskQuestion>,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// What one Orca poll found for an agent's terminal. Build it with [`RunUpdate::new`] and add the
/// transcript and the question the agent is waiting on with [`log`](RunUpdate::log) and
/// [`ask`](RunUpdate::ask).
#[derive(Debug, Clone, Copy)]
pub struct RunUpdate<'a> {
    pub task_id: &'a str,
    pub agent_id: &'a str,
    /// The Orca terminal status: `running`, `idle` or `exited`.
    pub status: &'a str,
    pub log: &'a str,
    pub phase: RunPhase,
    pub ask_prompt: &'a str,
    pub ask_options: &'a [String],
    pub ask_questions: &'a [AskQuestion],
}

impl<'a> RunUpdate<'a> {
    pub fn new(task_id: &'a str, agent_id: &'a str, status: &'a str, phase: RunPhase) -> RunUpdate<'a> {
        RunUpdate {
            task_id,
            agent_id,
            status,
            log: "",
            phase,
            ask_prompt: "",
            ask_options: &[],
            ask_questions: &[],
        }
    }

    /// The terminal transcript.
    pub fn log(mut self, log: &'a str) -> RunUpdate<'a> {
        self.log = log;
        self
    }

    /// The question the agent is waiting on and its answer options. Ignored while the phase is `Doing`.
    pub fn ask(mut self, prompt: &'a str, options: &'a [String]) -> RunUpdate<'a> {
        self.ask_prompt = prompt;
        self.ask_options = options;
        self
    }

    /// The questions block the agent is waiting on. Ignored while the phase is `Doing`.
    pub fn questions(mut self, questions: &'a [AskQuestion]) -> RunUpdate<'a> {
        self.ask_questions = questions;
        self
    }
}

impl Store {
    /// SaveTaskRun records the Orca terminal for one agent on a task.
    /// A new start replaces the previous terminal and log.
    pub fn save_task_run(&self, task_id: &str, agent_id: &str, terminal: &str) -> Result<()> {
        let agent_id = agent_id.trim();
        let terminal = terminal.trim();
        if task_id.is_empty() || agent_id.is_empty() || terminal.is_empty() {
            return Err(TaskError::new("Orca did not return a terminal for the agent.").into());
        }
        self.with_db(|db| {
            let tasks = TaskRepo::new(db);
            if !tasks.exists(task_id)? {
                return Err(ErrTaskNotFound.into());
            }
            RunRepo::new(db).start(task_id, agent_id, terminal, &now())?;
            if let Some(status) = tasks.agent_status(task_id, agent_id)? {
                tasks.set_agent_status(task_id, agent_id, status.start())?;
            }
            tasks.mark_started(task_id)?;
            tasks.sync_column(task_id)
        })
    }

    /// OpenRuns returns terminals that have not exited, oldest task first.
    pub fn open_runs(&self) -> Result<Vec<TaskRun>> {
        self.with_db(|db| RunRepo::new(db).open())
    }

    /// RecordTaskRun stores the latest Orca status, transcript, and question for one agent.
    /// The phase is Doing or Review. Completed stays Completed until the user approves.
    pub fn record_task_run(&self, update: &RunUpdate<'_>) -> Result<Task> {
        let agent_id = update.agent_id.trim();
        let status = update.status.trim();
        if update.task_id.is_empty() || agent_id.is_empty() || status.is_empty() {
            return Err(TaskError::new("Orca did not return a terminal for the agent.").into());
        }
        let (ask_prompt, ask_options, ask_questions) = if update.phase == RunPhase::Doing {
            ("", &[][..], &[][..])
        } else {
            (update.ask_prompt, update.ask_options, update.ask_questions)
        };
        let task_id = update.task_id;
        self.with_db(|db| {
            let runs = RunRepo::new(db);
            let tasks = TaskRepo::new(db);
            runs.update_log(task_id, agent_id, status, update.log, &now())?;
            runs.set_question(task_id, agent_id, ask_prompt, ask_options, ask_questions)?;
            if let Some(current) = tasks.agent_status(task_id, agent_id)? {
                let next = current.on_run_update(update.phase);
                if next != current {
                    tasks.set_agent_status(task_id, agent_id, next)?;
                }
            }
            tasks.sync_column(task_id)?;
            tasks.get(task_id)
        })
    }

    /// ResumeAgent sends an agent that is waiting for review back to work.
    pub fn resume_agent(&self, task_id: &str, agent_id: &str) -> Result<Task> {
        self.transition(task_id, agent_id, AgentStatus::resume, |db| {
            RunRepo::new(db).mark_running(task_id, agent_id.trim(), &now())
        })
    }

    /// ApproveAgent marks a waiting agent completed. The user is the only one who can do this.
    pub fn approve_agent(&self, task_id: &str, agent_id: &str) -> Result<Task> {
        self.transition(task_id, agent_id, AgentStatus::approve, |_| Ok(()))
    }

    /// Applies a user-driven status change to one agent, runs `after` for the side effects on its run,
    /// then moves the card. An agent that is not on the task fails the same way as one in the wrong state.
    fn transition(
        &self,
        task_id: &str,
        agent_id: &str,
        change: fn(AgentStatus) -> Result<AgentStatus, TaskError>,
        after: impl FnOnce(&rusqlite::Connection) -> Result<()>,
    ) -> Result<Task> {
        let agent_id = agent_id.trim();
        self.with_db(|db| {
            let tasks = TaskRepo::new(db);
            let current = tasks.agent_status(task_id, agent_id)?.unwrap_or(AgentStatus::Pending);
            let next = change(current)?;
            tasks.set_agent_status(task_id, agent_id, next)?;
            after(db)?;
            tasks.sync_column(task_id)?;
            tasks.get(task_id)
        })
    }
}
