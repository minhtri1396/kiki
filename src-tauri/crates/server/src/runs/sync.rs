//! Polling Orca: what each open terminal shows becomes the run status, transcript and question in the store.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use kiki_orca as orca;
use kiki_store as store;
use kiki_util::Ctx;

use super::status::{orca_run_status, RunStatus};
use crate::Server;

/// How long one Orca read may take.
const READ_TIMEOUT: Duration = Duration::from_secs(8);

/// What one poll found in an agent's terminal.
struct Observation {
    status: RunStatus,
    text: String,
    ask: orca::Ask,
}

impl Observation {
    fn of(view: &orca::TerminalView, idle: bool, run: &store::TaskRun) -> Observation {
        let (text, ask) = orca::transcript(&view.log);
        let mut status = orca_run_status(&view.status, idle, &run.started_at, Utc::now());
        if idle && !text.trim().is_empty() && view.status == "running" {
            status = RunStatus::Idle;
        }
        Observation { status, text, ask }
    }

    fn has_question(&self) -> bool {
        !self.ask.prompt.trim().is_empty()
    }
}

impl Server {
    /// Reads every open terminal and returns the tasks that changed, each once, in the order of their first run.
    pub(crate) fn sync_task_runs(&self, ctx: &Ctx) -> Result<Vec<store::Task>> {
        let mut order: Vec<String> = Vec::new();
        let mut latest: HashMap<String, store::Task> = HashMap::new();
        for run in &self.store.open_runs()? {
            if ctx.err().is_some() {
                break;
            }
            let Some(task) = self.refresh_run(ctx, run) else {
                continue;
            };
            if !latest.contains_key(&task.id) {
                order.push(task.id.clone());
            }
            latest.insert(task.id.clone(), task);
        }
        Ok(order.into_iter().filter_map(|id| latest.remove(&id)).collect())
    }

    /// Records one terminal. A terminal that cannot be read, or a task that cannot be stored, is skipped.
    fn refresh_run(&self, ctx: &Ctx, run: &store::TaskRun) -> Option<store::Task> {
        let view = self
            .orca
            .read_terminal(&ctx.with_timeout(READ_TIMEOUT), &run.terminal)
            .ok()?;
        let idle = view.status == "running"
            && self
                .orca
                .terminal_idle(&ctx.with_timeout(READ_TIMEOUT), &run.terminal)
                .unwrap_or(false);
        let seen = Observation::of(&view, idle, run);
        let (phase, prompt, options) = match seen.status.phase(seen.has_question()) {
            Some(phase) => (phase, seen.ask.prompt, seen.ask.options),
            None => self.current_phase(run)?,
        };
        let update = store::RunUpdate::new(&run.task_id, &run.agent_id, seen.status.as_str(), phase)
            .log(&seen.text)
            .ask(&prompt, &options);
        self.store.record_task_run(&update).ok()
    }

    /// The phase and question that leave the agent as it is, for a terminal whose status Orca could not tell.
    /// The store always takes a phase: an agent waiting for review (or completed) keeps its question, any
    /// other agent is working.
    fn current_phase(&self, run: &store::TaskRun) -> Option<(store::RunPhase, String, Vec<String>)> {
        let task = self.store.task(&run.task_id).ok()?;
        let waiting = task
            .agents
            .iter()
            .find(|agent| agent.agent_id == run.agent_id)
            .is_some_and(|agent| matches!(agent.status, store::AgentStatus::Review | store::AgentStatus::Completed));
        if !waiting {
            return Some((store::RunPhase::Doing, String::new(), Vec::new()));
        }
        let current = task.runs.iter().find(|stored| stored.agent_id == run.agent_id)?;
        Some((
            store::RunPhase::Review,
            current.ask_prompt.clone(),
            current.ask_options.clone(),
        ))
    }
}
