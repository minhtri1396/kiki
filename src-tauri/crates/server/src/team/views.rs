//! Tree comparison: lists the agents and repositories of a folder with a content hash, so the web UI can tell
//! which ones differ between this app, the user's folder and a teammate's folder.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use kiki_agentfile as agentfile;
use kiki_teamfolder as teamfolder;
use serde::Serialize;
use serde_json::{json, Value};

use super::files::hex_sha256;
use super::shared_repo::list_shared_repos;

/// One agent file in a listing.
#[derive(Serialize, Clone, Debug)]
pub(crate) struct TeamAgentView {
    pub id: String,
    pub name: String,
    pub category: String,
    pub hash: String,
}

/// One shared repository in a listing.
#[derive(Serialize, Clone, Debug)]
pub(crate) struct TeamRepoView {
    pub id: String,
    pub name: String,
    pub hash: String,
}

/// The display name of an agent file: its name, or its id when the name is blank.
pub(super) fn display_name(file: &agentfile::File) -> String {
    let name = file.name.trim();
    if name.is_empty() {
        file.id.clone()
    } else {
        name.to_string()
    }
}

/// The agent files under `dir`, each with the hash of its bytes.
pub(crate) fn agent_views(dir: &str) -> Result<Vec<TeamAgentView>> {
    let files = agentfile::list(dir)?;
    let mut views = Vec::with_capacity(files.len());
    for file in files {
        let raw = std::fs::read(&file.path).map_err(|e| anyhow!("open {}: {e}", file.path))?;
        views.push(TeamAgentView {
            name: display_name(&file),
            id: file.id,
            category: file.category,
            hash: hex_sha256(&raw),
        });
    }
    Ok(views)
}

/// The shared repositories under `dir`, each with the hash of its canonical text.
pub(crate) fn repo_views(dir: &str) -> Result<Vec<TeamRepoView>> {
    Ok(list_shared_repos(dir)?
        .into_iter()
        .map(|repo| TeamRepoView {
            hash: hex_sha256(repo.canonical().as_bytes()),
            name: repo.id.clone(),
            id: repo.id,
        })
        .collect())
}

/// True when both listings have the same ids with the same hashes.
pub(crate) fn same_repo_views(left: &[TeamRepoView], right: &[TeamRepoView]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let hashes: HashMap<&str, &str> = left.iter().map(|r| (r.id.as_str(), r.hash.as_str())).collect();
    right
        .iter()
        .all(|repo| hashes.get(repo.id.as_str()).is_some_and(|hash| *hash == repo.hash))
}

/// The member's repositories, and whether they differ from this app's.
fn compare_repos(local_repos: &str, member_dir: &str) -> Result<(Vec<TeamRepoView>, bool)> {
    let local = repo_views(local_repos)?;
    let member = repo_views(&teamfolder::repos_dir(member_dir))?;
    let same = same_repo_views(&local, &member);
    Ok((member, !same))
}

/// How one team folder compares with this app: its agents and repositories and whether either differs.
pub(crate) fn compare_folder(local_agents: &str, local_repos: &str, member_dir: &str, name: &str) -> Result<Value> {
    let mut views: Vec<TeamAgentView> = Vec::new();
    let mut differs = true;
    if teamfolder::has_agents(member_dir) {
        let agents = teamfolder::agents_dir(member_dir);
        views = agent_views(&agents)?;
        differs = !teamfolder::same(local_agents, &agents)?;
    }
    let (repos, repos_differ) = compare_repos(local_repos, member_dir)?;
    Ok(json!({
        "name": name,
        "differs": differs,
        "agents": views,
        "repos": repos,
        "reposDiffer": repos_differ,
    }))
}

/// A comparison with nothing in it: no member chosen, or (`missing`) a chosen member that has no folder.
pub(crate) fn empty_comparison(name: &str, missing: bool) -> Value {
    let mut view = json!({
        "name": name,
        "differs": false,
        "agents": Vec::<TeamAgentView>::new(),
        "repos": Vec::<TeamRepoView>::new(),
        "reposDiffer": false,
    });
    if missing {
        view["missing"] = Value::Bool(true);
    }
    view
}
