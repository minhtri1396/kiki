//! What the user's setup lets an agent use: MCP servers, tools, and other agents.

use super::model::Policy;

impl Policy {
    /// True when a selected MCP covers the server or tool `name`.
    /// Atlassian covers Jira and Confluence; each can also be selected on its own.
    pub(super) fn allows_mcp(&self, name: &str) -> bool {
        let name = name.trim().to_lowercase();
        if name.is_empty() {
            return false;
        }
        let product = if name.contains("confluence") {
            "confluence"
        } else if name.contains("jira") {
            "jira"
        } else {
            ""
        };
        if name.contains("atlassian") || !product.is_empty() {
            return self.allows_atlassian(product);
        }
        self.mcps.iter().any(|id| self.mcp_matches(&name, id))
    }

    /// `product` is `"jira"`, `"confluence"`, or empty when the name does not say which.
    fn allows_atlassian(&self, product: &str) -> bool {
        let has = |id: &str| listed(&self.mcps, id);
        if product.is_empty() {
            return has("atlassian") || has("atlassian_jira") || has("atlassian_confluence");
        }
        has("atlassian") || has(&format!("atlassian_{product}")) || has(product)
    }

    /// True when the selected MCP `id` is the server `name`, one of its words, or the workspace-scoped server key.
    fn mcp_matches(&self, name: &str, id: &str) -> bool {
        let id = id.trim().to_lowercase();
        if id.is_empty() || id == "atlassian" || id.starts_with("atlassian_") {
            return false;
        }
        if name == id || has_token(name, &id) {
            return true;
        }
        if self.workspace.is_empty() {
            return false;
        }
        let key = format!("mcp-{}-{}", id, self.workspace.to_lowercase());
        name == key || name.contains(&key)
    }

    /// True when the tool was selected. `gh` and "github cli" mean the GitHub CLI tool.
    pub(super) fn allows_tool(&self, name: &str) -> bool {
        let id = match name.trim().to_lowercase().as_str() {
            "gh" | "github_cli" | "github cli" => "github_cli",
            _ => name,
        };
        listed(&self.tools, id)
    }

    /// True for this agent itself and for the agents it may work with.
    pub(super) fn allows_agent(&self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        if name.to_lowercase() == self.agent_id.to_lowercase() {
            return true;
        }
        listed(&self.agents, name)
    }
}

/// Whether `id` is in `list`, ignoring case and surrounding spaces. A blank id is never listed.
fn listed(list: &[String], id: &str) -> bool {
    let id = id.trim().to_lowercase();
    if id.is_empty() {
        return false;
    }
    list.iter().any(|item| item.trim().to_lowercase() == id)
}

fn has_token(name: &str, id: &str) -> bool {
    name.split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|part| !part.is_empty())
        .any(|part| part == id)
}
