//! The data the guardrail works with.

use std::collections::HashMap;

use super::paths::abs_path;

/// What the user allowed for one agent.
/// `read_only` lists roots the agent may read and must not change.
/// `services` maps a repository root to the Nx projects that repository selected.
/// A file inside another Nx project under that root is blocked. A root with no
/// entry has no service limit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Policy {
    pub agent_id: String,
    pub workspace: String,
    pub mcps: Vec<String>,
    pub tools: Vec<String>,
    pub agents: Vec<String>,
    pub roots: Vec<String>,
    pub read_only: Vec<String>,
    pub services: HashMap<String, Vec<String>>,
    pub rtk: bool,
}

impl Policy {
    /// The policy with one more root (a worktree KIKI started the agent in). A read-only worktree is also
    /// read-only here; selected `services` limit it like they limit the repository.
    pub(super) fn with_root(mut self, extra: &str, readonly: bool, services: &[String]) -> Policy {
        let extra = abs_path(extra);
        if extra.is_empty() {
            return self;
        }
        self.roots.push(extra.clone());
        if readonly {
            self.read_only.push(extra.clone());
        }
        if services.is_empty() {
            return self;
        }
        self.services.insert(extra, services.to_vec());
        self
    }
}

/// What an action touches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TargetKind {
    /// An MCP server or one of its tools.
    Mcp,
    /// A tool such as the GitHub CLI or RTK.
    Tool,
    /// Another agent.
    Agent,
    /// Files and folders of a repository.
    #[default]
    Repo,
}

/// One concrete interaction found in a hook event.
/// `write` is true when a repository interaction changes files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    pub kind: TargetKind,
    pub name: String,
    pub paths: Vec<String>,
    pub write: bool,
}

impl Target {
    /// An MCP, tool, or agent target.
    pub fn named(kind: TargetKind, name: &str) -> Target {
        Target {
            kind,
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// A repository target.
    pub fn repo(paths: Vec<String>, write: bool) -> Target {
        Target {
            kind: TargetKind::Repo,
            paths,
            write,
            ..Default::default()
        }
    }
}

/// The guardrail decision. `allow` is false when the action is blocked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decision {
    pub allow: bool,
    pub message: String,
}

impl Decision {
    pub(super) fn allow() -> Decision {
        Decision {
            allow: true,
            message: String::new(),
        }
    }

    pub(super) fn block(message: String) -> Decision {
        Decision { allow: false, message }
    }
}

/// An agent KIKI started in a worktree.
/// `read_only` means that worktree belongs to a repository the agent must not change.
/// `services` are the Nx projects selected for that repository. Empty adds no service limit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Binding {
    pub agent_id: String,
    pub worktree: String,
    pub read_only: bool,
    pub services: Vec<String>,
}

/// The information available when a hook asks for a decision.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub cwd: String,
    pub agent: String,
    pub bindings: Vec<Binding>,
    pub policies: Vec<Policy>,
    pub workspace: Vec<String>,
}

/// How an agent was chosen for a hook.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    /// This folder is not one of the app's repositories.
    #[default]
    Outside,
    /// `Resolution::policy` is the agent whose setup decides.
    Use,
    /// The action is blocked before a policy is applied.
    Deny,
}

/// The agent selected for a hook, or the reason none can be used.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolution {
    pub status: Status,
    pub policy: Policy,
    pub message: String,
}

impl Resolution {
    pub(super) fn denied(message: String) -> Resolution {
        Resolution {
            status: Status::Deny,
            policy: Policy::default(),
            message,
        }
    }

    pub(super) fn used(policy: Policy) -> Resolution {
        Resolution {
            status: Status::Use,
            policy,
            message: String::new(),
        }
    }
}
