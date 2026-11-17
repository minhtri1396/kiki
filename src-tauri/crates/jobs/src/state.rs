//! The job table shared between the manager and the background runs, always accessed under one lock.

use crate::model::{Job, JobStatus, Report};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

const MAX_FINISHED_JOBS: usize = 30;
const MAX_LOG_BYTES: usize = 512 * 1024;
const KEPT_LOG_BYTES: usize = 400 * 1024;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) jobs: HashMap<String, Job>,
    /// Job ids, oldest first.
    pub(crate) order: Vec<String>,
    /// `tool:target` key to the id of the running job, so a second start returns the first.
    pub(crate) running: HashMap<String, String>,
    pub(crate) last: Report,
}

/// Locks the table; a poisoned lock is still usable because every update leaves it consistent.
pub(crate) fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// A copy of the job without its cancel handle.
pub(crate) fn snapshot(job: Option<&Job>) -> Job {
    match job {
        None => Job::default(),
        Some(job) => Job {
            cancel: None,
            ..job.clone()
        },
    }
}

impl State {
    /// Forgets the oldest finished jobs beyond the limit. Running jobs are always kept.
    pub(crate) fn trim(&mut self) {
        let (mut finished, running): (Vec<String>, Vec<String>) = self
            .order
            .iter()
            .cloned()
            .partition(|id| !matches!(self.jobs.get(id), Some(job) if job.status == JobStatus::Running));
        if finished.len() > MAX_FINISHED_JOBS {
            let cut = finished.len() - MAX_FINISHED_JOBS;
            for id in &finished[..cut] {
                self.jobs.remove(id);
            }
            finished.drain(..cut);
        }
        finished.extend(running);
        self.order = finished;
    }

    /// Appends to a job's log and keeps the log bounded.
    pub(crate) fn append_log(&mut self, id: &str, text: &str) {
        let Some(job) = self.jobs.get_mut(id) else { return };
        job.log.push_str(text);
        if job.log.len() > MAX_LOG_BYTES {
            let mut start = job.log.len() - KEPT_LOG_BYTES;
            while !job.log.is_char_boundary(start) {
                start += 1;
            }
            job.log = job.log[start..].to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(state: &mut State, id: &str, status: JobStatus) {
        state.jobs.insert(
            id.to_string(),
            Job {
                id: id.to_string(),
                status,
                ..Default::default()
            },
        );
        state.order.push(id.to_string());
    }

    #[test]
    fn trim_keeps_running_jobs_and_the_newest_finished() {
        let mut state = State::default();
        add(&mut state, "running", JobStatus::Running);
        for i in 0..35 {
            add(&mut state, &format!("done{i}"), JobStatus::Ok);
        }
        state.trim();
        assert_eq!(state.order.len(), 31);
        assert_eq!(state.order.last().unwrap(), "running");
        assert_eq!(state.order[0], "done5");
        assert!(!state.jobs.contains_key("done4") && state.jobs.contains_key("running"));
    }

    #[test]
    fn append_log_stays_bounded_on_char_boundaries() {
        let mut state = State::default();
        add(&mut state, "j", JobStatus::Running);
        let chunk = "é".repeat(MAX_LOG_BYTES / 2 + 10);
        state.append_log("j", &chunk);
        let log = &state.jobs["j"].log;
        assert!(log.len() <= KEPT_LOG_BYTES + 1 && log.chars().all(|c| c == 'é'));
        state.append_log("missing", "ignored");
    }
}
