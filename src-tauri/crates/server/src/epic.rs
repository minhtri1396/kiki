//! The Jira epic note added to a task's prompt.

use kiki_agentdef as agentdef;
use kiki_store as store;

use crate::server::MCP_JIRA;

/// Appends the Jira epic when this agent has Jira and the epic is on that agent's board.
pub(crate) fn epic_note(note: &str, task: &store::Task, agent: &agentdef::Agent) -> String {
    let (has_jira, board_id) = agent_jira_board(agent);
    let epic_board = task.epic_board_id.trim();
    let key = task.epic_key.trim();
    if !has_jira || key.is_empty() || epic_board.is_empty() || epic_board != board_id.trim() {
        return note.to_string();
    }
    let block = format!(
        "Jira epic: {}\nBoard: {}\nThis task belongs to this Jira epic. Do the work under this epic.\nIf you create Jira tickets that are not sub-tasks, create them under this epic.\nIf you create sub-tasks, create those sub-task tickets as children of the ticket this agent needs.",
        key,
        board_label(&task.epic_board_name, &task.epic_board_id)
    );
    let note = note.trim_end_matches('\n');
    if note.is_empty() {
        return block;
    }
    format!("{note}\n\n{block}")
}

fn agent_jira_board(agent: &agentdef::Agent) -> (bool, String) {
    for item in &agent.mcps {
        if item.id.trim() != MCP_JIRA {
            continue;
        }
        return (true, item.board_id.trim().to_string());
    }
    (false, String::new())
}

fn board_label(name: &str, id: &str) -> String {
    let name = name.trim();
    let id = id.trim();
    if name.is_empty() {
        return id.to_string();
    }
    if id.is_empty() || id == name {
        return name.to_string();
    }
    format!("{name} ({id})")
}
