//! The prompt sent to an agent when it starts.
//!
//! Pattern: Builder. `PromptBuilder` appends one section at a time (identity, repositories, indexes,
//! collaborators, MCP servers, tools, waiting notice, agent document, user message). Each section is one small
//! function in its own module, so a change to one section touches one file.

macro_rules! w {
    ($b:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        let _ = write!($b, $($arg)*);
    }};
}

mod identity;
mod indexes;
mod peers;
mod repos;
mod uses;
mod waiting;

#[cfg(test)]
mod tests;

use kiki_agentdef as agentdef;

use crate::orchestrator::orchestrator_label;
use crate::snapshot::{fill, Snapshot};

/// One agent this launch can send messages to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub handoffs: Vec<Handoff>,
}

/// One message for a peer. `when` is the condition for sending it and
/// `send` is the content. `reply` is what the peer reports back to the sender when it
/// has finished. An empty `reply` means no report. `max_loops`, when above zero, is the
/// most round trips the two agents may make.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Handoff {
    pub when: String,
    pub send: String,
    pub reply: String,
    pub max_loops: i64,
}

/// An agent that lists another agent under Agents Coordination and
/// so turned it on for a task.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Requester {
    pub id: String,
    pub title: String,
}

/// Builds the launch prompt one section at a time.
pub struct PromptBuilder<'a> {
    agent: &'a agentdef::Agent,
    snap: Snapshot,
    orchestrator: String,
    text: String,
}

impl<'a> PromptBuilder<'a> {
    /// Starts an empty prompt for `agent` in the setup `snap` (its folder paths are cleaned first).
    pub fn new(agent: &'a agentdef::Agent, snap: &Snapshot) -> Self {
        let snap = fill(snap.clone());
        let orchestrator = orchestrator_label(&snap.orchestrator);
        PromptBuilder {
            agent,
            snap,
            orchestrator,
            text: String::new(),
        }
    }

    /// Who the agent is, how it was started, and where the setup file is.
    pub fn identity(mut self) -> Self {
        identity::write(&mut self.text, self.agent, &self.snap, &self.orchestrator);
        self
    }

    /// The repositories this agent works in.
    pub fn repositories(mut self) -> Self {
        repos::write(&mut self.text, self.agent, &self.snap);
        self
    }

    /// The saved index (Obsidian or CodeGraph) of each repository.
    pub fn indexes(mut self) -> Self {
        indexes::write(&mut self.text, &self.agent.repos, &self.snap);
        self
    }

    /// The agent's own id and the messages it sends to other agents.
    pub fn collaborators(mut self, peers: &[Peer]) -> Self {
        peers::write(&mut self.text, &self.agent.id, peers, &self.orchestrator);
        self
    }

    /// The MCP servers this agent uses.
    pub fn mcp_servers(mut self) -> Self {
        uses::write(
            &mut self.text,
            "MCP servers for this agent:",
            "Use this MCP server:",
            &self.agent.mcps,
        );
        self
    }

    /// The tools this agent uses.
    pub fn tools(mut self) -> Self {
        uses::write(&mut self.text, "Tools for this agent:", "", &self.agent.tools);
        self
    }

    /// Says what to do first: wait for a request, wait for the user, or read the user message.
    pub fn startup_notice(mut self, requesters: &[Requester], note: &str) -> Self {
        waiting::write(&mut self.text, &self.agent.id, requesters, &self.orchestrator, note);
        self
    }

    /// The agent document, fenced.
    pub fn agent_document(mut self, body: &str) -> Self {
        w!(self.text, "===== AGENT DOCUMENT ({}) =====\n", self.agent.file);
        self.text.push_str(body);
        if !body.ends_with('\n') {
            self.text.push('\n');
        }
        self.text.push_str("===== END AGENT DOCUMENT =====\n");
        self
    }

    /// The user's message, fenced; nothing when it is empty.
    pub fn user_message(mut self, note: &str) -> Self {
        let note = note.trim();
        if !note.is_empty() {
            self.text.push_str("\n===== USER MESSAGE =====\n");
            self.text.push_str(note);
            self.text.push_str("\n===== END USER MESSAGE =====\n");
        }
        self
    }

    /// The finished prompt.
    pub fn build(self) -> String {
        self.text
    }
}

/// The text passed to the selected Agent Orchestration tool.
pub fn prompt(agent: &agentdef::Agent, body: &str, snap: &Snapshot, note: &str, peers: &[Peer]) -> String {
    prompt_waiting(agent, body, snap, note, peers, &[])
}

/// `prompt` for an agent that a task turned on because other agents
/// list it under Agents Coordination. That agent waits for a request from them
/// before it does any work.
pub fn prompt_waiting(
    agent: &agentdef::Agent,
    body: &str,
    snap: &Snapshot,
    note: &str,
    peers: &[Peer],
    requesters: &[Requester],
) -> String {
    PromptBuilder::new(agent, snap)
        .identity()
        .repositories()
        .indexes()
        .collaborators(peers)
        .mcp_servers()
        .tools()
        .startup_notice(requesters, note)
        .agent_document(body)
        .user_message(note)
        .build()
}
