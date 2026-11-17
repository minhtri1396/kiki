//! The Atlassian section of Settings (form and state) and installing the Atlassian MCP server for a launch.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use kiki_agentdef as agentdef;
use kiki_mcp as mcp;

use crate::server::{MCP_ATLASSIAN, MCP_CONFLUENCE, MCP_JIRA};
use crate::{LaunchPrep, Server};

/// The Atlassian section of the Settings form.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct AtlassianBody {
    pub enabled: bool,
    pub username: String,
    #[serde(rename = "apiToken")]
    pub api_token: String,
    pub jira: ServiceBody,
    pub confluence: ServiceBody,
}

/// One product in the Settings form.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ServiceBody {
    pub enabled: bool,
    pub url: String,
    #[serde(rename = "ownCredentials")]
    pub own: bool,
    pub username: String,
    #[serde(rename = "apiToken")]
    pub api_token: String,
    #[serde(rename = "boardId")]
    pub board_id: String,
    #[serde(rename = "boardName")]
    pub board_name: String,
    #[serde(rename = "spaceId")]
    pub space_id: String,
    #[serde(rename = "spaceKey")]
    pub space_key: String,
    #[serde(rename = "spaceName")]
    pub space_name: String,
    #[serde(rename = "parentId")]
    pub parent_id: String,
    #[serde(rename = "parentTitle")]
    pub parent_title: String,
}

impl AtlassianBody {
    pub(crate) fn form(&self) -> mcp::Atlassian {
        mcp::Atlassian {
            enabled: self.enabled,
            username: self.username.clone(),
            api_token: self.api_token.clone(),
            jira: self.jira.form(),
            confluence: self.confluence.form(),
        }
    }
}

impl ServiceBody {
    pub(crate) fn form(&self) -> mcp::Service {
        mcp::Service {
            enabled: self.enabled,
            url: self.url.clone(),
            own: self.own,
            username: self.username.clone(),
            api_token: self.api_token.clone(),
            board_id: self.board_id.clone(),
            board_name: self.board_name.clone(),
            space_id: self.space_id.clone(),
            space_key: self.space_key.clone(),
            space_name: self.space_name.clone(),
            parent_id: self.parent_id.clone(),
            parent_title: self.parent_title.clone(),
        }
    }
}

pub(crate) fn atlassian_state(cfg: &mcp::Atlassian) -> Value {
    json!({
        "enabled": cfg.enabled,
        "username": cfg.username,
        "apiToken": cfg.api_token,
        "apiTokenSet": !cfg.api_token.is_empty(),
        "jira": service_state(&cfg.jira),
        "confluence": service_state(&cfg.confluence),
    })
}

fn service_state(service: &mcp::Service) -> Value {
    json!({
        "enabled": service.enabled,
        "url": service.url,
        "ownCredentials": service.own,
        "username": service.username,
        "apiToken": service.api_token,
        "apiTokenSet": !service.api_token.is_empty(),
        "boardId": service.board_id,
        "boardName": service.board_name,
        "spaceId": service.space_id,
        "spaceKey": service.space_key,
        "spaceName": service.space_name,
        "parentId": service.parent_id,
        "parentTitle": service.parent_title,
    })
}

impl Server {
    /// Installs each MCP turned on for this agent into the AI tool selected for the run.
    /// An MCP the agent does not use is left out.
    pub(crate) fn ensure_agent_mcps(&self, prep: &LaunchPrep) -> Result<()> {
        if !agent_uses_atlassian(&prep.agent.mcps) {
            return Ok(());
        }
        self.ensure_atlassian_mcp(prep)
    }

    /// Installs Atlassian MCP under mcp-atlassian-<workspace> when the agent's AI tool does not already have that key.
    pub(crate) fn ensure_atlassian_mcp(&self, prep: &LaunchPrep) -> Result<()> {
        let cfg = self.store.atlassian()?;
        atlassian_ready_for(&cfg, &prep.agent.mcps)?;
        let home = self.hook_home()?;
        let mut worktree = prep.worktree.as_str();
        if !prep.task_id.is_empty() {
            // A feature run starts in a new checkout. Only the AI tool's own settings are sure to be there.
            worktree = "";
        }
        mcp::ensure(prep.tool, worktree, &home, &prep.snap.workspace_id, &cfg)
    }
}

pub(crate) fn atlassian_ready_for(cfg: &mcp::Atlassian, list: &[agentdef::Use]) -> Result<()> {
    if !cfg.ready() {
        return Err(anyhow!("set up Atlassian MCP before starting this agent"));
    }
    for item in list {
        match item.id.trim() {
            MCP_JIRA if !cfg.jira.enabled => {
                return Err(anyhow!("set up Atlassian - Jira before starting this agent"));
            }
            MCP_CONFLUENCE if !cfg.confluence.enabled => {
                return Err(anyhow!("set up Atlassian - Confluence before starting this agent"));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn agent_uses_atlassian(list: &[agentdef::Use]) -> bool {
    list.iter()
        .any(|item| matches!(item.id.trim(), MCP_ATLASSIAN | MCP_JIRA | MCP_CONFLUENCE))
}
