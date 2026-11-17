//! The user's side of a run: what a launch records, replying to a waiting agent and approving its work.

use std::time::Duration;

use anyhow::Result;
use kiki_orca as orca;
use kiki_store as store;
use kiki_util::Ctx;

use crate::api_error::{ApiError, ApiResult};
use crate::tasks::{task_error, user_error};
use crate::{LaunchPrep, Server};

/// How long Orca may take to type the reply into the terminal.
const REPLY_TIMEOUT: Duration = Duration::from_secs(20);

impl Server {
    /// Records the Orca terminal a launch started for a task agent. A launch without a task records nothing.
    pub(crate) fn remember_task_run(&self, prep: &LaunchPrep, raw: &str) -> Result<()> {
        if prep.task_id.trim().is_empty() {
            return Ok(());
        }
        if !orca::valid_task_id(&prep.task_id) {
            return Err(store::TaskError::new("Task ID is not valid.").into());
        }
        let handle = orca::terminal_handle(raw);
        if handle.is_empty() {
            return Err(store::TaskError::new("Orca did not return a terminal for the agent.").into());
        }
        self.store.save_task_run(&prep.task_id, &prep.agent.id, &handle)
    }

    /// Types the user's answer into the terminal of an agent that waits for review and sends it back to work.
    pub(crate) fn reply_to_agent(
        &self,
        ctx: &Ctx,
        task_id: &str,
        agent_id: &str,
        text: &str,
    ) -> ApiResult<store::Task> {
        let text = text.trim();
        if text.is_empty() {
            return Err(user_error("Enter a reply."));
        }
        let task = self.store.task(task_id).map_err(|err| task_error(&err))?;
        let run = review_run(&task, agent_id).map_err(|err| task_error(&err))?;
        self.orca
            .send_text(&ctx.with_timeout(REPLY_TIMEOUT), &run.terminal, text)
            .map_err(|err| ApiError::from_anyhow(502, &err))?;
        self.store
            .resume_agent(task_id, agent_id)
            .map_err(|err| task_error(&err))
    }

    /// Marks the work of a waiting agent as accepted.
    pub(crate) fn approve_task_agent(&self, task_id: &str, agent_id: &str) -> ApiResult<store::Task> {
        self.store
            .approve_agent(task_id, agent_id)
            .map_err(|err| task_error(&err))
    }
}

/// The run of an agent that waits for review, which is where a reply goes.
pub(crate) fn review_run(task: &store::Task, agent_id: &str) -> Result<store::TaskRun> {
    let waiting = task
        .agents
        .iter()
        .rfind(|agent| agent.agent_id == agent_id)
        .is_some_and(|agent| agent.status == store::AgentStatus::Review);
    if !waiting {
        return Err(store::TaskError::new("This agent is not waiting for review.").into());
    }
    match task
        .runs
        .iter()
        .find(|run| run.agent_id == agent_id && !run.terminal.trim().is_empty())
    {
        Some(run) => Ok(run.clone()),
        None => Err(store::TaskError::new("This agent has not been started.").into()),
    }
}
