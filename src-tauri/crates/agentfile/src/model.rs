//! The agent file model: one markdown file's fields, the typed errors callers branch on, and the cleanup
//! rules for handoffs and guide names that both the reader and the writer apply.

use std::fmt;

use anyhow::{anyhow, Result};
use kiki_agentdef as agentdef;

/// `ErrNotFound` means the agent id is not in the agent folder.
/// Test an `anyhow::Error` for it with `err.downcast_ref::<ErrNotFound>()` or [`is_not_found`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrNotFound;

impl fmt::Display for ErrNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown agent")
    }
}

impl std::error::Error for ErrNotFound {}

/// `ErrExists` means another agent file already uses that id.
/// Test an `anyhow::Error` for it with `err.downcast_ref::<ErrExists>()` or [`is_exists`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrExists;

impl fmt::Display for ErrExists {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an agent with that id already exists")
    }
}

impl std::error::Error for ErrExists {}

/// Reports whether `err` is (or wraps) [`ErrNotFound`].
pub fn is_not_found(err: &anyhow::Error) -> bool {
    err.chain().any(|e| e.downcast_ref::<ErrNotFound>().is_some())
}

/// Reports whether `err` is (or wraps) [`ErrExists`].
pub fn is_exists(err: &anyhow::Error) -> bool {
    err.chain().any(|e| e.downcast_ref::<ErrExists>().is_some())
}

/// One agent markdown file. `category` is the folder that contains it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct File {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub enabled: bool,
    pub ai_tool: String,
    pub model: String,
    pub repos: Vec<Repository>,
    pub works_with: Vec<Collaborator>,
    pub mcps: Vec<Use>,
    pub tools: Vec<Use>,
    pub prompt: String,
    pub path: String,
}

/// One repository this agent uses, and what it does there.
/// `readonly` means the agent may read that repository and must not change it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Repository {
    pub kind: String,
    pub guide: String,
    pub description: String,
    pub readonly: bool,
}

/// One agent this agent works with, and the messages this agent sends to it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Collaborator {
    pub id: String,
    pub handoffs: Vec<Handoff>,
}

/// One message for a collaborator. `when` is the condition for sending it.
/// `send` is the content to send. An empty `when` leaves the timing to the content.
/// `reply` is what the receiving agent reports back to this agent when it finishes.
/// An empty `reply` means it does not report back. `max_loops`, when above zero, means
/// the two agents may pass work back and forth, and it is the most round trips
/// allowed. `max_loops` only counts when `reply` is set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Handoff {
    pub when: String,
    pub send: String,
    pub reply: String,
    pub max_loops: i64,
}

/// One MCP server or tool this agent uses, and what it does with it.
/// Board and space fields are the Jira board or Confluence space and parent page
/// chosen for that MCP server. Tools leave them empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Use {
    pub id: String,
    pub description: String,
    pub board_id: String,
    pub board_name: String,
    pub space_id: String,
    pub space_key: String,
    pub space_name: String,
    pub parent_id: String,
    pub parent_title: String,
}

impl Use {
    /// True when the entry carries a note or a board/space choice, so it is written as a row and not as a bare id.
    pub(crate) fn detailed(&self) -> bool {
        !self.description.trim().is_empty()
            || !self.board_id.trim().is_empty()
            || !self.board_name.trim().is_empty()
            || !self.space_id.trim().is_empty()
            || !self.space_name.trim().is_empty()
            || !self.parent_id.trim().is_empty()
            || !self.parent_title.trim().is_empty()
    }
}

/// The most round trips a message may allow.
pub const MAX_HANDOFF_LOOPS: i64 = 20;

/// Reports a message that has no condition or nothing to send. The
/// agent editor requires both. It is not part of `save`, so an older file with a note
/// and no condition can still be saved by the rest of KIKI.
pub fn check_handoffs(list: &[Collaborator]) -> Result<()> {
    for peer in list {
        for (i, item) in peer.handoffs.iter().enumerate() {
            if item.when.trim().is_empty() {
                return Err(anyhow!(
                    "enter Send when for message {} to {}, or remove the message",
                    i + 1,
                    peer.id.trim()
                ));
            }
            if item.send.trim().is_empty() {
                return Err(anyhow!(
                    "enter What to send for message {} to {}, or remove the message",
                    i + 1,
                    peer.id.trim()
                ));
            }
        }
    }
    Ok(())
}

/// Trims every handoff, drops the empty ones, and keeps `max_loops` only where there is a reply.
pub(crate) fn clean_handoffs(list: &[Handoff]) -> Vec<Handoff> {
    let mut out = Vec::new();
    for item in list {
        let when = item.when.trim();
        let send = item.send.trim();
        if when.is_empty() && send.is_empty() {
            continue;
        }
        let mut handoff = Handoff {
            when: when.to_string(),
            send: send.to_string(),
            reply: item.reply.trim().to_string(),
            max_loops: 0,
        };
        if !handoff.reply.is_empty() {
            handoff.max_loops = item.max_loops;
        }
        out.push(handoff);
    }
    out
}

/// The guide file name when it is not the default, and empty when
/// the agent reads the default guide file.
pub(crate) fn custom_guide(name: &str) -> String {
    let name = name.trim();
    if name == agentdef::DEFAULT_GUIDE {
        return String::new();
    }
    name.to_string()
}
