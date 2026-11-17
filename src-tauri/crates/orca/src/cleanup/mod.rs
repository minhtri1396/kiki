//! Removing the Orca state of a finished task: terminals, worktrees, project setups and the group folder.

#[cfg(test)]
mod tests;

use crate::backend::Backend;
use crate::groups::Groups;
use crate::names::{feature_branch, feature_project_name, valid_branch_slug};
use crate::projects::Projects;
use crate::wire::{json_ok, orca_gone};
use crate::worktrees::worktree_named;
use anyhow::{anyhow, Result};
use kiki_util::Ctx;
use std::collections::{BTreeMap, HashSet};

/// TaskScope is the Orca state created for one Kiki task.
/// ID is the worktree name. GroupID is the sidebar folder.
/// Folders maps a repo kind to that repo's folder.
/// KeepGroup leaves the folder in place. KeepRepos leaves those projects in place.
#[derive(Clone, Debug, Default)]
pub struct TaskScope {
    pub id: String,
    pub group_id: String,
    pub workspace: String,
    pub slug: String,
    pub ticket: String,
    pub repos: Vec<String>,
    pub folders: BTreeMap<String, String>,
    pub terminals: Vec<String>,
    pub keep_group: bool,
    pub keep_repos: Vec<String>,
}

/// The distinct non-blank entries, trimmed, in order.
fn distinct_trimmed(items: &[String]) -> Vec<&str> {
    let mut seen: HashSet<&str> = HashSet::new();
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty() && seen.insert(item))
        .collect()
}

pub(crate) struct Cleanup {
    backend: Backend,
}

impl Cleanup {
    pub(crate) fn new(backend: Backend) -> Cleanup {
        Cleanup { backend }
    }

    /// Closes the task's agent terminals and removes its worktrees.
    /// The group folder and repo projects stay when another task in the group still uses them.
    pub(crate) fn remove_task(&self, ctx: &Ctx, scope: &TaskScope) -> Result<()> {
        self.backend.cli.ensure_running(ctx)?;
        for handle in distinct_trimmed(&scope.terminals) {
            self.run_gone(ctx, &["terminal", "close", "--terminal", handle, "--tab", "--json"])?;
        }
        let keep: HashSet<&str> = scope
            .keep_repos
            .iter()
            .map(|kind| kind.trim())
            .filter(|k| !k.is_empty())
            .collect();
        for kind in distinct_trimmed(&scope.repos) {
            self.remove_repo(ctx, scope, kind, keep.contains(kind))?;
        }
        if scope.keep_group || scope.group_id.trim().is_empty() {
            return Ok(());
        }
        Groups::new(self.backend.clone()).delete(ctx, &scope.group_id)
    }

    /// Removes the task worktree of one repo and, unless kept, that repo's project setup in the group.
    fn remove_repo(&self, ctx: &Ctx, scope: &TaskScope, kind: &str, keep_project: bool) -> Result<()> {
        let branch = if valid_branch_slug(&scope.slug) {
            feature_branch(&scope.ticket, &scope.slug, kind)?
        } else {
            String::new()
        };
        let root = scope.folders.get(kind).cloned().unwrap_or_default();
        self.remove_task_worktree(ctx, &root, &scope.id, &branch)?;
        if keep_project || scope.group_id.trim().is_empty() {
            return Ok(());
        }
        let name = feature_project_name(&scope.workspace, &scope.group_id, kind)?;
        let setups = Projects::new(self.backend.clone()).setups(ctx)?;
        for setup in setups
            .iter()
            .filter(|s| s.display_name == name && !s.id.trim().is_empty())
        {
            self.run_gone(
                ctx,
                &[
                    "project",
                    "setup-delete",
                    "--setup",
                    setup.id.as_str(),
                    "--force",
                    "--json",
                ],
            )?;
        }
        Ok(())
    }

    /// Removes the worktree by path when it can be found, otherwise by branch, otherwise by name.
    fn remove_task_worktree(&self, ctx: &Ctx, root: &str, name: &str, branch: &str) -> Result<()> {
        let root = root.trim();
        let name = name.trim();
        if !root.is_empty() && !name.is_empty() {
            let listed = self.backend.cli.command_output(
                ctx,
                &["worktree", "list", "--repo", format!("path:{root}").as_str(), "--json"],
            );
            if let Ok(listed) = listed {
                let path = worktree_named(&listed, name);
                if !path.is_empty() {
                    return self.remove_worktree(ctx, &format!("path:{path}"));
                }
            }
        }
        if !branch.is_empty() {
            return self.remove_worktree(ctx, &format!("branch:{branch}"));
        }
        if !name.is_empty() {
            return self.remove_worktree(ctx, &format!("name:{name}"));
        }
        Ok(())
    }

    fn remove_worktree(&self, ctx: &Ctx, selector: &str) -> Result<()> {
        self.run_gone(ctx, &["worktree", "rm", "--worktree", selector, "--json"])
    }

    /// Runs a removal command; "already gone" counts as success.
    fn run_gone<S: AsRef<str>>(&self, ctx: &Ctx, args: &[S]) -> Result<()> {
        let out = self.backend.cli.command_output(ctx, args)?;
        if json_ok(&out) || orca_gone(&out) {
            return Ok(());
        }
        Err(anyhow!("orca {} failed\n{}", args[0].as_ref(), crate::cli::trim(&out)))
    }
}
