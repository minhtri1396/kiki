//! An agent file as a guardrail policy: the MCPs, tools, peers and repository folders it may use.

use std::collections::HashSet;
use std::path::Path;

use kiki_agentfile as agentfile;
use kiki_console as console;
use kiki_guard as kg;
use kiki_util::path;

/// The policy of one agent. `enabled` lists the tools switched on in Settings; `rtk` is the Settings flag.
pub(super) fn policy_from(
    file: &agentfile::File,
    snap: &console::Snapshot,
    enabled: &HashSet<String>,
    rtk: bool,
) -> kg::Policy {
    let mut policy = kg::Policy {
        agent_id: file.id.clone(),
        workspace: snap.workspace_id.clone(),
        mcps: use_ids(&file.mcps),
        tools: enabled_use_ids(&file.tools, enabled),
        rtk,
        ..Default::default()
    };
    policy.agents = file
        .works_with
        .iter()
        .map(|peer| peer.id.trim())
        .filter(|id| !id.is_empty())
        .map(String::from)
        .collect();
    for repo in &file.repos {
        let setup = snap.repo(&repo.kind);
        let folder = setup.folder.trim();
        if !Path::new(folder).is_absolute() {
            continue;
        }
        let mut folder = path::clean(folder);
        if !setup.sparse.is_empty() {
            folder = path::join(&[&folder, &setup.sparse]);
        }
        policy.roots.push(folder.clone());
        if repo.readonly {
            policy.read_only.push(folder.clone());
        }
        if !setup.services.is_empty() {
            policy.services.insert(folder, setup.services.clone());
        }
    }
    policy
}

/// The absolute, cleaned folders of the saved repositories.
pub(super) fn repo_folders(snap: &console::Snapshot) -> Vec<String> {
    snap.repos
        .iter()
        .map(|repo| repo.folder.trim())
        .filter(|folder| Path::new(folder).is_absolute())
        .map(path::clean)
        .collect()
}

fn enabled_use_ids(list: &[agentfile::Use], enabled: &HashSet<String>) -> Vec<String> {
    use_ids(list).into_iter().filter(|id| enabled.contains(id)).collect()
}

/// The non-empty ids of an agent's MCP or tool list.
pub(crate) fn use_ids(list: &[agentfile::Use]) -> Vec<String> {
    list.iter()
        .map(|item| item.id.trim())
        .filter(|id| !id.is_empty())
        .map(String::from)
        .collect()
}
