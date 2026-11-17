//! The Agents Coordination section: the agent's own id and the messages it sends to other agents.

use super::{Handoff, Peer};

pub(super) fn write(b: &mut String, myself: &str, peers: &[Peer], orchestrator: &str) {
    let myself = myself.trim();
    let list = clean_peers(myself, peers);
    if myself.is_empty() && list.is_empty() {
        return;
    }
    if !myself.is_empty() {
        w!(b, "Your agent id is {}. Other agents reach you by this id.\n", myself);
    }
    if list.is_empty() {
        b.push('\n');
        return;
    }
    b.push_str(
        "Agents Coordination with these agents. When you communicate or hand work to one of them, use that agent id.\n",
    );
    w!(b, "Each message below has a condition and the content to send. While you work, check each condition. When a condition is met, send that content to the agent through {}, addressed by its agent id. Add the facts the other agent needs, such as paths, ids, and results. Send each message when its condition is met, not before. A message with no condition is sent at the point its content describes.\n", orchestrator);
    for peer in &list {
        write_peer(b, myself, peer, orchestrator);
    }
    b.push('\n');
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
    w!(b, "- agent id: {}\n", peer.id);
    if peer.handoffs.is_empty() {
        b.push_str("  No message is set for this agent.\n");
        return;
    }
    for (i, pair) in peer.handoffs.iter().enumerate() {
        write_handoff(b, myself, &peer.id, pair, i + 1, orchestrator);
    }
}

fn write_handoff(b: &mut String, myself: &str, peer: &str, pair: &Handoff, number: usize, orchestrator: &str) {
    w!(b, "  Message {} to {}:\n", number, peer);
    if pair.when.is_empty() {
        b.push_str("    Send when: (no condition set)\n");
    } else {
        b.push_str("    Send when:\n");
        write_lines(b, "      ", &pair.when);
    }
    b.push_str("    Send:\n");
    write_lines(b, "      ", &pair.send);
    write_report_back(b, myself, peer, pair, orchestrator);
    if !pair.reply.is_empty() && pair.max_loops > 0 {
        write_loop(b, peer, pair, number);
    }
}

/// Whether the peer reports back, and what that report must contain.
fn write_report_back(b: &mut String, myself: &str, peer: &str, pair: &Handoff, orchestrator: &str) {
    if pair.reply.is_empty() {
        w!(b, "    Report back: no. Do not ask {} to report back.\n", peer);
        return;
    }
    if myself.is_empty() {
        w!(
            b,
            "    Report back: yes. In this message, ask {} to report back to you when it has finished the work.\n",
            peer
        );
    } else {
        w!(b, "    Report back: yes. In this message, ask {} to report back to you, agent id {}, when it has finished the work. Use {} for that report. Expect that report.\n", peer, myself, orchestrator);
    }
    w!(b, "    The report from {} must contain:\n", peer);
    write_lines(b, "      ", &pair.reply);
    b.push_str("    Put that in the message so the other agent knows what to send back.\n");
}

/// States the loop limit for one message. A loop is one round trip, where
/// the agent sends the message and the other agent reports back.
fn write_loop(b: &mut String, peer: &str, pair: &Handoff, number: usize) {
    let mut again = format!("the report from {peer} asks for more work, or shows the work is not done");
    if !pair.when.is_empty() {
        again = format!("the Send when condition of message {number} is met again after the report from {peer}");
    }
    w!(b, "    Loop: this exchange can go back and forth between you and {peer}. Count one loop each time you send this message and {peer} reports back. Allow at most {max} loops. Send this message again only when {again}. State the loop number in every message you send, for example \"loop 1 of {max}\". After loop {max}, do not send this message again: stop the exchange, and tell the user what is still open.\n", max = pair.max_loops);
}

/// Each line of `text`, indented.
fn write_lines(b: &mut String, indent: &str, text: &str) {
    for line in text.split('\n') {
        w!(b, "{}{}\n", indent, line);
    }
}
