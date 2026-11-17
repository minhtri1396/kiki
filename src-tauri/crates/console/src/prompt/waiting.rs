//! What the agent does first: wait for a request, wait for the user, or read the user message.

use super::Requester;

pub(super) fn write(b: &mut String, myself: &str, requesters: &[Requester], orchestrator: &str, note: &str) {
    let has_note = !note.trim().is_empty();
    if write_waiting(b, myself, requesters, orchestrator, has_note) {
        return;
    }
    if has_note {
        b.push_str("The user sent a message with this launch. It is repeated after the agent document. Treat that message as the user message in your terminal.\n\n");
    } else {
        b.push_str("The user did not send a ticket, a requirement folder, or any other instruction. Follow the Startup behavior in the agent document and wait.\n\n");
    }
}

/// Tells a supporting agent to stay idle until a request reaches it.
/// It reports whether it wrote anything.
fn write_waiting(b: &mut String, myself: &str, requesters: &[Requester], orchestrator: &str, has_note: bool) -> bool {
    let list = requester_names(myself, requesters);
    if list.is_empty() {
        return false;
    }
    w!(b, "WAIT FOR A REQUEST. KIKI turned you on for a task because you are listed under Agents Coordination by {}. You are a supporting agent. Do not start any work yet. Do not read repositories, do not create or edit files, and do not run project commands until a request reaches you. Stay idle and wait for a request from {}, sent through {} to your agent id. Do the work in the agent document only after that request arrives, and do it for what the request asks.\n", list.join(" and "), list.join(" or "), orchestrator);
    if has_note {
        b.push_str("The user message at the end of this prompt is the task. It is context for the request that will reach you. It is not an instruction to start now.\n");
    }
    b.push('\n');
    true
}

/// "id (title)" for each requester other than the agent itself.
fn requester_names(myself: &str, requesters: &[Requester]) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    for item in requesters {
        let id = item.id.trim();
        if id.is_empty() || id == myself.trim() {
            continue;
        }
        let title = item.title.trim();
        if title.is_empty() {
            list.push(id.to_string());
        } else {
            list.push(format!("{id} ({title})"));
        }
    }
    list
}
