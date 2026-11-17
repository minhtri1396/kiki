//! The opening of the prompt: who the agent is and where the setup lives.

use kiki_agentdef as agentdef;

use crate::orchestrator::{normalize_orchestrator, orchestrator_label};
use crate::snapshot::Snapshot;

pub(super) fn write(b: &mut String, agent: &agentdef::Agent, snap: &Snapshot, orchestrator: &str) {
    w!(b, "You are {} ({}).\n\n", agent.id, agent.title);
    write_launch_lead(b, snap);
    w!(b, "Repository root: {}\n", snap.root);
    w!(b, "{} worktree: {}\n", orchestrator, snap.worktree);
    w!(b, "Setup file: {}\n", snap.setup_file);
    w!(b, "Workspace id: {}\n", snap.workspace_id);
    b.push_str("\nThe setup file is .kiki/kiki.yaml next to the app. The app folder is the repository root. Its absolute path is the Setup file line above. Read that file in Step 0. It is the only setup file. The workspace id is workspace_id in that file. Do not invent values that are missing, and do not assume the workspace id is kiki. Folder paths and service names below are the saved setup.\n\n");
    w!(b, "You were started in the {} worktree. That folder is the repository for this launch. Run git and project commands in the repository folder from the setup file. Do not create, edit, commit, or run project commands in a repository that is not listed. A repository with no folder is not set up.\n\n", orchestrator);
    b.push_str("Service names are the Nx project names in a code repository. Use them when you choose a project and its guide file. The document repository has a folder and no service names.\n\n");
}

/// Names the Agent Orchestration tool that started this agent.
/// Orca is told it was started with `orca agent launch`.
fn write_launch_lead(b: &mut String, snap: &Snapshot) {
    match normalize_orchestrator(&snap.orchestrator).as_str() {
        "orca" => b.push_str("KIKI started you with `orca agent launch`. Follow the agent document at the end of this prompt exactly. That document is your operating instructions.\n\n"),
        _ => w!(b, "KIKI started you with {}. Follow the agent document at the end of this prompt exactly. That document is your operating instructions.\n\n", orchestrator_label(&snap.orchestrator)),
    }
}
