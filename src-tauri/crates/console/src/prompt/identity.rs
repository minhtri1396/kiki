//! The opening of the prompt: who the agent is and where the setup lives.

use kiki_agentdef as agentdef;

use crate::orchestrator::{normalize_orchestrator, orchestrator_label};
use crate::snapshot::Snapshot;

pub(super) fn write(b: &mut String, agent: &agentdef::Agent, snap: &Snapshot, orchestrator: &str) {
    w!(b, "You are {} ({}).\n\n", agent.id, agent.title);
    write_launch_lead(b, snap);
    w!(b, "Folder root: {}\n", snap.root);
    let folders = repository_folders(agent, snap);
    if folders.len() > 1 {
        w!(b, "{} worktrees:\n", orchestrator);
        for (kind, folder) in &folders {
            w!(b, "- {kind}: {folder}\n");
        }
    } else {
        w!(b, "{} worktree: {}\n", orchestrator, snap.worktree);
    }
    w!(b, "Setup file: {}\n", snap.setup_file);
    w!(b, "Workspace id: {}\n", snap.workspace_id);
    b.push_str("\nThe setup file is .kiki/kiki.yaml next to the app. The app folder is the Folder root line above. The setup file path is the Setup file line above. This is the only setup file. The workspace id is workspace_id in that file. Do not invent values that are missing, and do not assume the workspace id is kiki. Folder paths and service names below are the saved setup.\n\n");
    write_where_started(b, orchestrator, folders.len() > 1);
    b.push_str("When a repository lists services, those names are the Nx projects in that repository. Use a listed service when you choose a project. A service uses its own guide files when they are set, and the repository guide files when it shares them. A repository with no services line is not divided into services.\n\n");
}

/// One line per repository this agent uses: its kind and the saved folder.
fn repository_folders(agent: &agentdef::Agent, snap: &Snapshot) -> Vec<(String, String)> {
    agent
        .repos
        .iter()
        .filter_map(|item| {
            let kind = item.kind.trim();
            if kind.is_empty() {
                return None;
            }
            let saved = snap.repo(kind);
            let folder = saved.folder.trim();
            let folder = if folder.is_empty() { "(not set)" } else { folder };
            Some((kind.to_string(), folder.to_string()))
        })
        .collect()
}

/// Where this process started, and which folders are the repositories for the launch.
fn write_where_started(b: &mut String, orchestrator: &str, several: bool) {
    if several {
        w!(b, "Each repository has its own {} worktree, listed above. Run git and project commands in that repository's worktree. Do not share one worktree across repositories. Do not create, edit, commit, or run project commands in a repository that is not listed. A repository with no folder is not set up.\n\n", orchestrator);
        return;
    }
    w!(b, "You were started in the {} worktree. That folder is the repository for this launch. Run git and project commands in the repository folder from the setup file. Do not create, edit, commit, or run project commands in a repository that is not listed. A repository with no folder is not set up.\n\n", orchestrator);
}

/// Names the Agent Orchestration tool that started this agent.
/// Orca is told it was started with `orca agent launch`.
fn write_launch_lead(b: &mut String, snap: &Snapshot) {
    match normalize_orchestrator(&snap.orchestrator).as_str() {
        "orca" => b.push_str("KIKI started you with `orca agent launch`. Follow the agent document at the end of this prompt exactly. That document is your operating instructions.\n\n"),
        _ => w!(b, "KIKI started you with {}. Follow the agent document at the end of this prompt exactly. That document is your operating instructions.\n\n", orchestrator_label(&snap.orchestrator)),
    }
}
