//! The catalog of KIKI agents embedded in the binary.
//! The documents live in `kiki-agents` and are compiled in by cargo.
//!
//! `catalog` lists the agents (built with a small builder), `peers`, `mcps` and `repos` hold the notes
//! written into each agent's launch prompt.

use anyhow::{anyhow, Result};

mod catalog;
mod mcps;
mod peers;
mod repos;
#[cfg(test)]
mod tests;

pub use catalog::CATALOG;

/// The guide file an agent reads in a repository when none is set.
pub const DEFAULT_GUIDE: &str = "AGENTS.md";

/// The guide file name to use. An empty name is `DEFAULT_GUIDE`.
pub fn guide_file(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return DEFAULT_GUIDE.to_string();
    }
    name.to_string()
}

/// Reports whether `name` is a markdown file name with no folder in it.
pub fn valid_guide(name: &str) -> bool {
    let name = guide_file(name);
    if name.contains(['/', '\\', '\r', '\n', '\0']) {
        return false;
    }
    let b = name.as_bytes();
    b.len() > ".md".len() && b[b.len() - ".md".len()..].eq_ignore_ascii_case(b".md")
}

/// One repository this agent uses.
/// `guide` is the markdown file the agent reads and follows when it works in that
/// repository. Empty means `DEFAULT_GUIDE`.
/// `description` is the note written into the agent file when KIKI creates it.
/// `readonly` means this agent may read the repository and must not change it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Repo {
    pub kind: String,
    pub guide: String,
    pub description: String,
    pub readonly: bool,
}

/// One agent this agent coordinates with.
/// `collaboration` is the note written into the agent file when KIKI creates it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Peer {
    pub id: String,
    pub collaboration: String,
}

/// One MCP server or tool this agent uses.
/// `name` is the label shown in the launch prompt. `description` is what the agent does with it.
/// Board and space fields are the Jira board or Confluence space and parent page for that server.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Use {
    pub id: String,
    pub name: String,
    pub description: String,
    pub board_id: String,
    pub board_name: String,
    pub space_id: String,
    pub space_key: String,
    pub space_name: String,
    pub parent_id: String,
    pub parent_title: String,
}

/// One embedded instruction document the console can start.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Agent {
    pub id: String,
    pub title: String,
    pub group: String,
    pub file: String,
    pub repos: Vec<Repo>,
    pub peers: Vec<Peer>,
    pub mcps: Vec<Use>,
    pub tools: Vec<Use>,
    pub summary: String,
}

/// Returns the catalog entry for an agent id.
pub fn by_id(id: &str) -> Option<Agent> {
    CATALOG.iter().find(|a| a.id == id).cloned()
}

impl Agent {
    /// Returns the embedded markdown for this agent.
    pub fn body(&self) -> Result<String> {
        let raw = kiki_agents::file(&self.file).ok_or_else(|| {
            anyhow!(
                "read embedded agent {}: open {}: file does not exist",
                self.id,
                self.file
            )
        })?;
        let text = String::from_utf8_lossy(raw).into_owned();
        if !text.contains("# Agent:") {
            return Err(anyhow!("embedded agent {} has no document heading", self.id));
        }
        if text.contains("## Works with agents") || text.contains("**Agent ID:**") {
            return Err(anyhow!(
                "embedded agent {} repeats fields the launch prompt already includes",
                self.id
            ));
        }
        if text.contains("obsidian vault=") || text.contains("codegraph explore") || text.contains("codegraph_explore")
        {
            return Err(anyhow!(
                "embedded agent {} contains index instructions the launch prompt adds",
                self.id
            ));
        }
        Ok(text)
    }
}
