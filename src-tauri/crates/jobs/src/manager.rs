//! The manager: the shared job table and the public start/list/cancel operations.

use crate::env::Env;
use crate::index::{self, IndexTool, Launch, Setup};
use crate::model::{Job, JobStatus, Report};
use crate::runner::Run;
use crate::state::{lock, snapshot, State};
use crate::text::{format_args, prefix_log};
use anyhow::{anyhow, Result};
use kiki_util::Ctx;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const JOB_TIMEOUT: Duration = Duration::from_secs(45 * 60);

/// Manager runs setup commands. It is shared between threads: wrap it in an
/// `Arc` and call its methods through `&self`.
pub struct Manager {
    pub(crate) state: Arc<Mutex<State>>,
    pub(crate) env: Env,
}

impl Default for Manager {
    fn default() -> Self {
        Manager::new()
    }
}

impl Manager {
    /// New returns an empty manager.
    pub fn new() -> Manager {
        Manager {
            state: Arc::new(Mutex::new(State::default())),
            env: Env::default(),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Last returns the most recent status check.
    pub fn last(&self) -> Report {
        self.lock().last.clone()
    }

    /// List returns jobs, newest first.
    pub fn list(&self) -> Vec<Job> {
        let st = self.lock();
        st.order.iter().rev().map(|id| snapshot(st.jobs.get(id))).collect()
    }

    /// Get returns one job.
    pub fn get(&self, id: &str) -> Option<Job> {
        let st = self.lock();
        st.jobs.get(id).map(|job| snapshot(Some(job)))
    }

    /// Cancel stops a running job.
    pub fn cancel(&self, id: &str) -> bool {
        let st = self.lock();
        match st.jobs.get(id) {
            Some(job) if job.status == JobStatus::Running => match &job.cancel {
                Some(ctx) => {
                    ctx.cancel();
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    /// StartObsidian makes one repository folder an Obsidian vault, then checks the CLI.
    pub fn start_obsidian(&self, kind: &str, folder: &str) -> Result<Job> {
        self.setup(index::obsidian(), Setup { root: "", kind, folder })
    }

    /// StartCodegraph runs codegraph init for one repository folder.
    pub fn start_codegraph(&self, root: &str, kind: &str, folder: &str) -> Result<Job> {
        self.setup(index::codegraph(), Setup { root, kind, folder })
    }

    /// The shared setup steps: validate the request, let the tool prepare, then launch in the background.
    fn setup(&self, tool: &'static dyn IndexTool, req: Setup<'_>) -> Result<Job> {
        if !kiki_toolchain::repo_id(req.kind) {
            return Err(anyhow!("invalid repo"));
        }
        if req.folder.trim().is_empty() {
            return Err(anyhow!("folder is not set"));
        }
        let launch = tool.prepare(&self.env, &req)?;
        match self.start(tool, req.kind, &launch) {
            Ok(job) => Ok(job),
            Err(err) => Err(tool.start_error(&launch, err)),
        }
    }

    /// Starts `launch` in the background for `target`, or returns the job already running for it.
    pub(crate) fn start(&self, tool: &'static dyn IndexTool, target: &str, launch: &Launch) -> Result<Job> {
        let name = &launch.args[0];
        let bin = (self.env.look_path)(name).map_err(|_| anyhow!("command {name} was not found on PATH"))?;
        let key = format!("{}:{target}", tool.name());
        let mut st = self.lock();
        if let Some(id) = st.running.get(&key) {
            return Ok(snapshot(st.jobs.get(id)));
        }
        let ctx = Ctx::background().with_timeout(JOB_TIMEOUT).with_cancel();
        let job = new_job(tool, target, launch, &ctx);
        let id = job.id.clone();
        let snap = snapshot(Some(&job));
        st.jobs.insert(id.clone(), job);
        st.order.push(id.clone());
        st.running.insert(key.clone(), id.clone());
        st.trim();
        let mut argv = vec![bin];
        argv.extend(launch.args[1..].iter().cloned());
        let run = Run {
            state: Arc::clone(&self.state),
            ctx,
            id,
            key,
            dir: launch.dir.clone(),
            args: argv,
            tool,
        };
        std::thread::spawn(move || run.execute());
        Ok(snap)
    }
}

fn new_job(tool: &dyn IndexTool, target: &str, launch: &Launch, ctx: &Ctx) -> Job {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Job {
        id: format!("{}-{target}-{nanos}", tool.name()),
        kind: tool.name().to_string(),
        target: target.to_string(),
        status: JobStatus::Running,
        command: format_args(&launch.args),
        log: prefix_log(&launch.note),
        started_at: kiki_util::now_rfc3339(),
        cancel: Some(ctx.clone()),
        ..Default::default()
    }
}
