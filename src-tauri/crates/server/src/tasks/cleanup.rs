//! Removing a task from Orca: its terminals, worktrees and, when no other task needs them, its group folder.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Result;
use kiki_agentfile as agentfile;
use kiki_orca as orca;
use kiki_store as store;
use kiki_util::Ctx;

use super::branch::task_ticket;
use super::error::task_error;
use crate::api_error::{ApiError, ApiResult};
use crate::Server;

/// The text Orca reports when its app is closed. A task that never started has nothing to clean up then.
const ORCA_OFFLINE: &str = "Orca is not running";

/// True when `err` says the Orca app is not running.
pub(crate) fn orca_is_offline(err: &anyhow::Error) -> bool {
    format!("{err:#}").contains(ORCA_OFFLINE)
}

/// A task has used Orca once it started or got a branch name.
pub(crate) fn uses_orca(task: &store::Task) -> bool {
    task.started || !task.branch_slug.trim().is_empty()
}

impl Server {
    /// Deletes a task. A task that used Orca is removed from Orca first; the task stays when Orca refuses,
    /// except for a task that never started while the Orca app is closed.
    pub(crate) fn delete_task(&self, ctx: &Ctx, id: &str) -> ApiResult<()> {
        let task = self.store.task(id).map_err(|err| task_error(&err))?;
        if uses_orca(&task) {
            if let Err(err) = self.remove_task_from_orca(ctx, &task) {
                if task.started || !orca_is_offline(&err) {
                    return Err(ApiError::from_anyhow(502, &err));
                }
            }
        }
        self.store.delete_task(id).map_err(|err| task_error(&err))
    }

    pub(crate) fn remove_task_from_orca(&self, ctx: &Ctx, task: &store::Task) -> Result<()> {
        let snap = self.load()?;
        let kinds = self.task_repo_kinds(task)?;
        let (keep_group, keep_repos) = self.orca_group_still_used(task)?;
        let folders: BTreeMap<String, String> = kinds
            .iter()
            .map(|kind| (kind.clone(), snap.repo(kind).folder.trim().to_string()))
            .collect();
        let scope = orca::TaskScope {
            id: task.id.clone(),
            group_id: task.group_id.clone(),
            workspace: snap.workspace_id.clone(),
            slug: task.branch_slug.clone(),
            ticket: task_ticket(task),
            repos: kinds,
            folders,
            terminals: run_terminals(task),
            keep_group,
            keep_repos,
        };
        self.orca.remove_task(ctx, &scope)
    }

    /// Whether another started task of the same group still needs the Orca group, and which repositories.
    pub(crate) fn orca_group_still_used(&self, task: &store::Task) -> Result<(bool, Vec<String>)> {
        let group_id = task.group_id.trim();
        if group_id.is_empty() {
            return Ok((false, Vec::new()));
        }
        let mut keep_group = false;
        let mut seen: HashSet<String> = HashSet::new();
        let mut keep_repos: Vec<String> = Vec::new();
        for other in self.store.tasks()? {
            if other.id == task.id || other.group_id.trim() != group_id || !uses_orca(&other) {
                continue;
            }
            keep_group = true;
            for kind in self.task_repo_kinds(&other)? {
                if seen.insert(kind.clone()) {
                    keep_repos.push(kind);
                }
            }
        }
        Ok((keep_group, keep_repos))
    }

    /// Every repository kind the task's agents use, without repeats. Each kind has its own worktree.
    pub(crate) fn task_repo_kinds(&self, task: &store::Task) -> Result<Vec<String>> {
        let dir = self.agent_dir()?;
        let files = agentfile::list(&dir)?;
        let by_id: HashMap<&str, &agentfile::File> = files.iter().map(|file| (file.id.as_str(), file)).collect();
        let mut kinds: Vec<String> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for agent in &task.agents {
            let Some(file) = by_id.get(agent.agent_id.as_str()) else {
                continue;
            };
            for repo in &file.repos {
                let kind = repo.kind.trim();
                if !kind.is_empty() && seen.insert(kind) {
                    kinds.push(kind.to_string());
                }
            }
        }
        Ok(kinds)
    }
}

/// The distinct terminal handles of the task's runs, in run order.
fn run_terminals(task: &store::Task) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut terminals: Vec<String> = Vec::new();
    for run in &task.runs {
        let handle = run.terminal.trim();
        if !handle.is_empty() && seen.insert(handle) {
            terminals.push(handle.to_string());
        }
    }
    terminals
}
