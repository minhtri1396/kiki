//! Orca worktrees: finding or creating the worktree of a task, and checking it belongs to the repository.

use crate::backend::Backend;
use crate::paths::{abs_path, same_path};
use crate::wire::{nd, parse, Envelope};
use anyhow::{anyhow, Result};
use kiki_util::{path, Ctx};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct WorktreeRow {
    #[serde(deserialize_with = "nd")]
    path: String,
    #[serde(deserialize_with = "nd")]
    branch: String,
    #[serde(rename = "displayName", deserialize_with = "nd")]
    display_name: String,
}

/// The `result` of `worktree list` and `worktree create`: a list, a single worktree, or the bare path.
#[derive(Deserialize, Default)]
#[serde(default)]
struct WorktreeResult {
    #[serde(deserialize_with = "nd")]
    path: String,
    #[serde(deserialize_with = "nd")]
    branch: String,
    #[serde(deserialize_with = "nd")]
    worktree: WorktreeRow,
    #[serde(deserialize_with = "nd")]
    worktrees: Vec<WorktreeRow>,
}

fn worktree_rows(out: &[u8]) -> Vec<WorktreeRow> {
    let Ok(env) = parse::<Envelope<WorktreeResult>>(out) else {
        return Vec::new();
    };
    let result = env.result;
    let mut rows = result.worktrees;
    if !result.worktree.path.is_empty() {
        rows.push(result.worktree);
    }
    if !result.path.is_empty() {
        rows.push(WorktreeRow {
            path: result.path,
            branch: result.branch,
            display_name: String::new(),
        });
    }
    rows
}

/// The folder of the worktree on `branch`; with no branch, the only worktree in the reply.
fn worktree_path(out: &[u8], branch: &str) -> String {
    let rows = worktree_rows(out);
    if branch.is_empty() {
        return match rows.as_slice() {
            [only] => only.path.clone(),
            _ => String::new(),
        };
    }
    rows.into_iter()
        .find(|row| !row.path.is_empty() && branch_matches(&row.branch, branch))
        .map(|row| row.path)
        .unwrap_or_default()
}

/// The folder of the worktree whose display name is `name`.
pub(crate) fn worktree_named(out: &[u8], name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return String::new();
    }
    worktree_rows(out)
        .into_iter()
        .find(|row| !row.path.is_empty() && row.display_name.trim() == name)
        .map(|row| row.path)
        .unwrap_or_default()
}

fn branch_matches(got: &str, want: &str) -> bool {
    let got = got.strip_prefix("refs/heads/").unwrap_or(got);
    let want = want.strip_prefix("refs/heads/").unwrap_or(want);
    !got.is_empty() && got == want
}

pub(crate) struct Worktrees {
    backend: Backend,
}

impl Worktrees {
    pub(crate) fn new(backend: Backend) -> Worktrees {
        Worktrees { backend }
    }

    /// The folder of the worktree named `name` for the repo at `root`, creating it on `branch` when missing.
    pub(crate) fn ensure(&self, ctx: &Ctx, root: &str, project: &str, name: &str, branch: &str) -> Result<String> {
        let cli = &self.backend.cli;
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("worktree name is required"));
        }
        let listed = cli.run(
            ctx,
            &["worktree", "list", "--repo", format!("path:{root}").as_str(), "--json"],
        )?;
        let existing = worktree_named(&listed, name);
        if !existing.is_empty() {
            return Ok(existing);
        }
        let created = cli.run(
            ctx,
            &[
                "worktree",
                "create",
                "--project",
                project,
                "--name",
                name,
                "--branch",
                branch,
                "--no-parent",
                "--json",
            ],
        )?;
        let found = [
            worktree_named(&created, name),
            worktree_path(&created, branch),
            worktree_path(&created, ""),
        ]
        .into_iter()
        .find(|path| !path.is_empty());
        found.ok_or_else(|| anyhow!("Orca created worktree {name} but did not return its folder"))
    }
}

/// Reports that checkout is a worktree of the git repository at root.
pub(crate) fn same_git_repo(root: &str, checkout: &str) -> Result<()> {
    let repo = git_common_dir(root).map_err(|_| anyhow!("repository folder is not a git checkout"))?;
    let work = git_common_dir(checkout).map_err(|_| anyhow!("worktree folder is not a git checkout"))?;
    if !same_path(&repo, &work) {
        return Err(anyhow!("worktree is not a checkout of this repository"));
    }
    Ok(())
}

fn git_common_dir(dir: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["-C", dir, "rev-parse", "--git-common-dir"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!("git rev-parse failed: {}", crate::cli::exit_text(&out.status)));
    }
    let mut common = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if common.is_empty() {
        return Err(anyhow!("git common dir is empty"));
    }
    if !path::is_abs(&common) {
        common = path::join(&[dir, &common]);
    }
    Ok(path::clean(&abs_path(&common)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command as Cmd;

    #[test]
    fn same_git_repo_cases() {
        let root = tempfile::tempdir().unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let out = Cmd::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_AUTHOR_NAME", "kiki")
                .env("GIT_AUTHOR_EMAIL", "kiki@example.com")
                .env("GIT_COMMITTER_NAME", "kiki")
                .env("GIT_COMMITTER_EMAIL", "kiki@example.com")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}\n{}{}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        };
        let root_path = root.path();
        git(root_path, &["init"]);
        git(root_path, &["commit", "--allow-empty", "-m", "init"]);
        let checkout_parent = tempfile::tempdir().unwrap();
        let checkout = checkout_parent.path().join("task");
        git(
            root_path,
            &["worktree", "add", "-b", "feat/sample", checkout.to_str().unwrap()],
        );
        same_git_repo(root_path.to_str().unwrap(), checkout.to_str().unwrap()).unwrap();
        let other = tempfile::tempdir().unwrap();
        git(other.path(), &["init"]);
        assert!(
            same_git_repo(root_path.to_str().unwrap(), other.path().to_str().unwrap()).is_err(),
            "a different repository was accepted"
        );
        let missing = same_git_repo("/definitely/not/a/repo", "/also/not")
            .unwrap_err()
            .to_string();
        assert_eq!(missing, "repository folder is not a git checkout");
    }

    #[test]
    fn worktree_replies() {
        let list = br#"{"ok":true,"result":{"worktrees":[{"path":"/w/a","branch":"refs/heads/feat/a","displayName":" 12 "},{"path":"","displayName":"13"}]}}"#;
        assert_eq!(worktree_named(list, "12"), "/w/a");
        assert_eq!(worktree_named(list, "13"), "");
        assert_eq!(worktree_named(list, " "), "");
        assert_eq!(worktree_path(list, "feat/a"), "/w/a");
        assert_eq!(worktree_path(list, ""), "", "two rows are ambiguous");
        let one = br#"{"ok":true,"result":{"path":"/w/b","branch":"feat/b"}}"#;
        assert_eq!(worktree_path(one, ""), "/w/b");
        assert_eq!(worktree_path(one, "refs/heads/feat/b"), "/w/b");
        assert!(worktree_rows(b"junk").is_empty());
    }
}
