//! Which repository folder an agent works in.

use anyhow::{anyhow, Result};

use kiki_agentdef as agentdef;
use kiki_console as console;

/// The repo kind and folder an agent works in. `allow_unset` lets a preview run on a repo without a folder.
pub(crate) fn worktree_of(
    agent: &agentdef::Agent,
    snap: &console::Snapshot,
    kind: &str,
    allow_unset: bool,
) -> Result<(String, String)> {
    let kind = match (kind.is_empty(), agent.repos.as_slice()) {
        (false, _) => kind.to_string(),
        (true, [only]) => only.kind.clone(),
        (true, _) => return Err(anyhow!("choose the repo folder for this agent")),
    };
    if !agent.repos.iter().any(|item| item.kind == kind) {
        return Err(anyhow!("that repo is not used by this agent"));
    }
    let folder = snap.repo(&kind).folder.trim().to_string();
    if folder.is_empty() {
        if allow_unset {
            return Ok((kind, String::new()));
        }
        return Err(anyhow!("{kind} folder is not set"));
    }
    Ok((kind, folder))
}

/// Whether the agent may only read the repository of `kind`.
pub(crate) fn repo_readonly(repos: &[agentdef::Repo], kind: &str) -> bool {
    let kind = kind.trim();
    repos
        .iter()
        .find(|repo| repo.kind == kind)
        .map(|repo| repo.readonly)
        .unwrap_or(false)
}
