//! The agent form (`POST /api/agents` and `PUT /api/agents/{id}`) and its conversion into an agent file.

use serde::Deserialize;

use kiki_agentfile as agentfile;
use kiki_tools as tools;

use crate::http::null_default;

#[derive(Deserialize, Default)]
#[serde(default)]
struct RepoForm {
    kind: String,
    guide: String,
    description: String,
    readonly: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct HandoffForm {
    when: String,
    send: String,
    reply: String,
    #[serde(rename = "maxLoops")]
    max_loops: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PeerForm {
    id: String,
    #[serde(deserialize_with = "null_default")]
    handoffs: Vec<HandoffForm>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct McpForm {
    id: String,
    description: String,
    #[serde(rename = "boardId")]
    board_id: String,
    #[serde(rename = "boardName")]
    board_name: String,
    #[serde(rename = "spaceId")]
    space_id: String,
    #[serde(rename = "spaceKey")]
    space_key: String,
    #[serde(rename = "spaceName")]
    space_name: String,
    #[serde(rename = "parentId")]
    parent_id: String,
    #[serde(rename = "parentTitle")]
    parent_title: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ToolForm {
    id: String,
}

/// The body the web UI sends when an agent is saved.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct AgentForm {
    name: String,
    description: String,
    category: String,
    enabled: bool,
    pub(super) tool: String,
    pub(super) model: String,
    #[serde(deserialize_with = "null_default")]
    repos: Vec<RepoForm>,
    #[serde(rename = "worksWith", deserialize_with = "null_default")]
    works_with: Vec<PeerForm>,
    #[serde(deserialize_with = "null_default")]
    mcps: Vec<McpForm>,
    #[serde(deserialize_with = "null_default")]
    tools: Vec<ToolForm>,
    prompt: String,
}

impl AgentForm {
    /// `{snake_case(category)}_{snake_case(name)}` for a new agent. Empty when either part is blank.
    pub(super) fn created_id(&self) -> String {
        agentfile::new_id(&self.category, &self.name)
    }

    /// The collaborators, checked for complete handoffs.
    pub(super) fn collaborators(&self) -> anyhow::Result<Vec<agentfile::Collaborator>> {
        let list: Vec<agentfile::Collaborator> = self
            .works_with
            .iter()
            .map(|peer| agentfile::Collaborator {
                id: peer.id.clone(),
                handoffs: peer
                    .handoffs
                    .iter()
                    .map(|pair| agentfile::Handoff {
                        when: pair.when.clone(),
                        send: pair.send.clone(),
                        reply: pair.reply.clone(),
                        max_loops: pair.max_loops,
                    })
                    .collect(),
            })
            .collect();
        agentfile::check_handoffs(&list)?;
        Ok(list)
    }

    fn repositories(&self) -> Vec<agentfile::Repository> {
        self.repos
            .iter()
            .map(|repo| agentfile::Repository {
                kind: repo.kind.clone(),
                description: repo.description.clone(),
                readonly: repo.readonly,
                ..Default::default()
            })
            .collect()
    }

    fn mcp_uses(&self) -> Vec<agentfile::Use> {
        self.mcps
            .iter()
            .map(|item| agentfile::Use {
                id: item.id.clone(),
                description: item.description.clone(),
                board_id: item.board_id.clone(),
                board_name: item.board_name.clone(),
                space_id: item.space_id.clone(),
                space_key: item.space_key.clone(),
                space_name: item.space_name.clone(),
                parent_id: item.parent_id.clone(),
                parent_title: item.parent_title.clone(),
            })
            .collect()
    }

    /// The chosen tools; global tools apply to every agent and are not saved per agent.
    fn tool_uses(&self) -> Vec<agentfile::Use> {
        self.tools
            .iter()
            .map(|item| item.id.trim())
            .filter(|id| !id.is_empty() && !tools::global(id))
            .map(|id| agentfile::Use {
                id: id.to_string(),
                ..Default::default()
            })
            .collect()
    }

    /// The agent file to save under `id`, with the already resolved model and collaborators.
    pub(super) fn into_file(
        self,
        id: &str,
        model: String,
        works_with: Vec<agentfile::Collaborator>,
    ) -> agentfile::File {
        agentfile::File {
            id: id.to_string(),
            repos: self.repositories(),
            mcps: self.mcp_uses(),
            tools: self.tool_uses(),
            name: self.name,
            description: self.description,
            category: self.category,
            enabled: self.enabled,
            ai_tool: self.tool,
            model,
            works_with,
            prompt: self.prompt,
            ..Default::default()
        }
    }
}
