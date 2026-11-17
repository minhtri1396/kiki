//! Pattern: Template Method. `Run::execute` fixes the order of a background job: spawn the command, stream its
//! output into the job log, then finish as cancelled, failed or ok. The one step that differs per tool is the hook
//! `IndexTool::judge_success`.

use crate::index::IndexTool;
use crate::model::JobStatus;
use crate::process::{run_combined, CommandError};
use crate::state::{lock, State};
use crate::text::decode_chunk;
use kiki_util::{Ctx, CtxErr};
use std::sync::{Arc, Mutex};

/// Everything one background run needs; it owns the job until the command ends.
pub(crate) struct Run {
    pub(crate) state: Arc<Mutex<State>>,
    pub(crate) ctx: Ctx,
    pub(crate) id: String,
    /// The `tool:target` key in the running table.
    pub(crate) key: String,
    pub(crate) dir: String,
    /// The resolved binary followed by its arguments.
    pub(crate) args: Vec<String>,
    pub(crate) tool: &'static dyn IndexTool,
}

impl Run {
    /// The template: stream the command, flush the log, record the outcome.
    pub(crate) fn execute(self) {
        let outcome = self.stream_output();
        self.finish(outcome);
    }

    /// Spawns the command and appends its output to the job log as it arrives.
    fn stream_output(&self) -> Result<(), CommandError> {
        let mut pending: Vec<u8> = Vec::new();
        let outcome = run_combined(&self.ctx, &self.args[0], &self.args[1..], &self.dir, |chunk| {
            let text = decode_chunk(&mut pending, chunk);
            if !text.is_empty() {
                lock(&self.state).append_log(&self.id, &text);
            }
        });
        if !pending.is_empty() {
            lock(&self.state).append_log(&self.id, &String::from_utf8_lossy(&pending));
        }
        outcome
    }

    /// Releases the running slot and sets the final status: cancelled, error, or ok (as judged by the tool).
    fn finish(&self, outcome: Result<(), CommandError>) {
        let mut st = lock(&self.state);
        st.running.remove(&self.key);
        let Some(job) = st.jobs.get_mut(&self.id) else { return };
        job.finished_at = kiki_util::now_rfc3339();
        if self.ctx.err() == Some(CtxErr::Canceled) {
            job.status = JobStatus::Cancelled;
            job.error = "cancelled".to_string();
        } else if let Err(err) = outcome {
            let text = err.to_string();
            job.status = JobStatus::Error;
            job.error = text.clone();
            if job.log.trim().is_empty() {
                job.log = text;
            }
        } else if let Some(problem) = self.tool.judge_success(&job.log) {
            job.status = JobStatus::Error;
            job.error = problem;
        } else {
            job.status = JobStatus::Ok;
        }
    }
}
