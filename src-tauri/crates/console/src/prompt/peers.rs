//! The Agents Coordination section: the agent's own id and the messages it sends to other agents.
//!
//! The launch prompt is also shown to the user as Markdown, so this section uses headings, labels,
//! and blank lines. The words the agent documents look up stay in the text: Send when, Send,
//! Report back: yes, Report back: no, and Loop.

use super::{Handoff, Peer};

pub(super) fn write(b: &mut String, myself: &str, peers: &[Peer], orchestrator: &str) {
    let myself = myself.trim();
    let list = clean_peers(myself, peers);
    if myself.is_empty() && list.is_empty() {
        return;
    }
    if !myself.is_empty() {
        w!(
            b,
            "Your agent id is `{}`. Other agents reach you by this id.\n\n",
            myself
        );
    }
    if list.is_empty() {
        return;
    }
    b.push_str("## Agents Coordination\n\n");
    b.push_str(
        "Coordinate with these agents. When you communicate or hand work to one of them, use that agent id.\n\n",
    );
    b.push_str("Each message below has a condition and the content to send.\n\n");
    b.push_str("- While you work, check each condition.\n");
    w!(
        b,
        "- When a condition is met, send that content to the agent through {}, addressed by its agent id.\n",
        orchestrator
    );
    b.push_str("- Add the facts the other agent needs, such as paths, ids, and results.\n");
    b.push_str("- Send each message when its condition is met, not before.\n");
    b.push_str("- A message with no condition is sent at the point its content describes.\n\n");
    for peer in &list {
        write_peer(b, myself, peer, orchestrator);
    }
}

/// The peers worth writing: no empty id, not the agent itself, and only messages that have content.
/// A loop limit only counts for a message that asks for a report.
fn clean_peers(myself: &str, peers: &[Peer]) -> Vec<Peer> {
    let mut list: Vec<Peer> = Vec::with_capacity(peers.len());
    for peer in peers {
        let id = peer.id.trim();
        if id.is_empty() || id == myself {
            continue;
        }
        let handoffs = peer.handoffs.iter().filter_map(clean_handoff).collect();
        list.push(Peer {
            id: id.to_string(),
            handoffs,
        });
    }
    list
}

fn clean_handoff(pair: &Handoff) -> Option<Handoff> {
    let send = pair.send.trim();
    if send.is_empty() {
        return None;
    }
    let reply = pair.reply.trim().to_string();
    let max_loops = if !reply.is_empty() && pair.max_loops > 0 {
        pair.max_loops
    } else {
        0
    };
    Some(Handoff {
        when: pair.when.trim().to_string(),
        send: send.to_string(),
        reply,
        max_loops,
    })
}

fn write_peer(b: &mut String, myself: &str, peer: &Peer, orchestrator: &str) {
    w!(b, "### {}\n\n", peer.id);
    if peer.handoffs.is_empty() {
        b.push_str("No message is set for this agent.\n\n");
        return;
    }
    for (i, pair) in peer.handoffs.iter().enumerate() {
        write_handoff(b, myself, &peer.id, pair, i + 1, orchestrator);
    }
}

fn write_handoff(b: &mut String, myself: &str, peer: &str, pair: &Handoff, number: usize, orchestrator: &str) {
    w!(b, "#### Message {} to {}\n\n", number, peer);
    if pair.when.is_empty() {
        b.push_str("**Send when:** (no condition set)\n\n");
    } else {
        b.push_str("**Send when**\n\n");
        write_lines(b, &pair.when);
    }
    b.push_str("**Send**\n\n");
    write_lines(b, &pair.send);
    write_report_back(b, myself, peer, pair, orchestrator);
    if !pair.reply.is_empty() && pair.max_loops > 0 {
        write_loop(b, peer, pair, number);
    }
}

/// Whether the peer reports back, and what that report must contain.
fn write_report_back(b: &mut String, myself: &str, peer: &str, pair: &Handoff, orchestrator: &str) {
    if pair.reply.is_empty() {
        w!(b, "**Report back: no.** Do not ask {} to report back.\n\n", peer);
        return;
    }
    b.push_str("**Report back: yes**\n\n");
    if myself.is_empty() {
        w!(
            b,
            "- In this message, ask {} to report back to you when it has finished the work.\n\n",
            peer
        );
    } else {
        w!(
            b,
            "- In this message, ask {} to report back to you, agent id {}, when it has finished the work.\n",
            peer,
            myself
        );
        w!(b, "- Use {} for that report.\n", orchestrator);
        b.push_str("- Expect that report.\n\n");
    }
    w!(b, "**The report from {} must contain:**\n\n", peer);
    write_lines(b, &pair.reply);
    b.push_str("Put that in the message so the other agent knows what to send back.\n\n");
}

/// States the loop limit for one message. A loop is one round trip, where
/// the agent sends the message and the other agent reports back.
fn write_loop(b: &mut String, peer: &str, pair: &Handoff, number: usize) {
    let again = if pair.when.is_empty() {
        format!("the report from {peer} asks for more work, or shows the work is not done")
    } else {
        format!("the Send when condition of message {number} is met again after the report from {peer}")
    };
    b.push_str("**Loop**\n\n");
    w!(b, "This exchange can go back and forth between you and {peer}.\n\n");
    w!(
        b,
        "- Count one loop each time you send this message and {peer} reports back.\n"
    );
    w!(b, "- Allow at most {} loops.\n", pair.max_loops);
    w!(b, "- Send this message again only when {again}.\n");
    w!(
        b,
        "- State the loop number in every message you send, for example \"loop 1 of {}\".\n",
        pair.max_loops
    );
    w!(
        b,
        "- After loop {}, do not send this message again: stop the exchange, and tell the user what is still open.\n\n",
        pair.max_loops
    );
}

/// Each line of `text` as its own paragraph, so the prompt view keeps the line breaks.
fn write_lines(b: &mut String, text: &str) {
    for line in text.split('\n') {
        if line.is_empty() {
            continue;
        }
        b.push_str(line);
        b.push_str("\n\n");
    }
}
