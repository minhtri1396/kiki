//! The MCP servers and tools Settings offers to agents, and how an agent's own picks are matched against them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use kiki_agentdef as agentdef;
use kiki_agentfile as agentfile;
use kiki_mcp as mcp;
use kiki_tools as tools;

/// The saved choice that meant "all of Atlassian" before Jira and Confluence were listed apart.
pub(crate) const MCP_ATLASSIAN: &str = "atlassian";
pub(crate) const MCP_JIRA: &str = "atlassian_jira";
pub(crate) const MCP_CONFLUENCE: &str = "atlassian_confluence";

/// One MCP server or tool the edit-agent dialog can offer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedId {
    pub id: String,
    pub name: String,
}

impl NamedId {
    fn new(id: &str, name: &str) -> NamedId {
        NamedId {
            id: id.to_string(),
            name: name.to_string(),
        }
    }
}

/// Fills a Jira board or Confluence space from Settings when this agent has not chosen its own.
pub(crate) fn apply_atlassian_places(mut uses: Vec<agentdef::Use>, cfg: &mcp::Atlassian) -> Vec<agentdef::Use> {
    let cfg = cfg.normalized();
    for item in uses.iter_mut() {
        match item.id.trim() {
            MCP_JIRA if item.board_id.trim().is_empty() && item.board_name.trim().is_empty() => {
                item.board_id = cfg.jira.board_id.clone();
                item.board_name = cfg.jira.board_name.clone();
            }
            MCP_CONFLUENCE if item.space_id.trim().is_empty() && item.space_name.trim().is_empty() => {
                item.space_id = cfg.confluence.space_id.clone();
                item.space_key = cfg.confluence.space_key.clone();
                item.space_name = cfg.confluence.space_name.clone();
                item.parent_id = cfg.confluence.parent_id.clone();
                item.parent_title = cfg.confluence.parent_title.clone();
            }
            _ => {}
        }
    }
    uses
}

/// The MCP servers Settings turned on.
pub(crate) fn mcps_from_atlassian(cfg: &mcp::Atlassian) -> Vec<NamedId> {
    let cfg = cfg.normalized();
    let mut out: Vec<NamedId> = Vec::new();
    if !cfg.enabled {
        return out;
    }
    if cfg.jira.enabled {
        out.push(NamedId::new(MCP_JIRA, "Atlassian - Jira"));
    }
    if cfg.confluence.enabled {
        out.push(NamedId::new(MCP_CONFLUENCE, "Atlassian - Confluence"));
    }
    out
}

/// The catalog tools Settings turned on (global tools are left out).
pub(crate) fn tools_from(cfg: &tools::Settings) -> Vec<NamedId> {
    tools::catalog()
        .into_iter()
        .filter(|item| !item.global && cfg.on(&item.id))
        .map(|item| NamedId {
            id: item.id,
            name: item.name,
        })
        .collect()
}

/// Reports that every tool selected for the agent is turned on in Settings.
pub(crate) fn tools_ready(wanted: &[String], enabled: &[NamedId]) -> Result<()> {
    let ready: BTreeSet<&str> = enabled.iter().map(|item| item.id.as_str()).collect();
    let missing: Vec<&str> = wanted
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty() && !tools::global(id) && !ready.contains(id))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(anyhow!("set up {} before starting this agent", missing.join(", ")))
}

/// `[{id, name}]` for the web UI; an empty name shows the id.
pub(crate) fn named_json(list: &[NamedId]) -> Vec<Value> {
    list.iter()
        .filter(|item| !item.id.trim().is_empty())
        .map(|item| {
            let id = item.id.trim();
            let name = item.name.trim();
            json!({ "id": id, "name": if name.is_empty() { id } else { name } })
        })
        .collect()
}

/// Every catalog tool with its Settings state, for the Settings page.
pub(crate) fn tool_choice_json(cfg: &tools::Settings) -> Vec<Value> {
    tools::catalog()
        .into_iter()
        .map(|item| {
            json!({
                "id": item.id,
                "name": item.name,
                "command": item.command,
                "detail": item.detail,
                "page": item.page,
                "enabled": cfg.on(&item.id),
                "installed": tools::installed(&item.command),
                "global": item.global,
            })
        })
        .collect()
}

/// The agent's picks that Settings still offers, each once, with the Settings name.
pub(crate) fn catalog_uses(list: &[agentfile::Use], allowed: &[NamedId]) -> Vec<agentdef::Use> {
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for item in allowed {
        let id = item.id.trim();
        if id.is_empty() {
            continue;
        }
        let name = item.name.trim();
        names.insert(id.to_string(), if name.is_empty() { id } else { name }.to_string());
    }
    let mut out: Vec<agentdef::Use> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for item in &spread_atlassian(list, &names) {
        let id = item.id.trim();
        let Some(name) = names.get(id) else { continue };
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        out.push(agentdef::Use {
            id: id.to_string(),
            name: name.clone(),
            description: item.description.clone(),
            board_id: item.board_id.clone(),
            board_name: item.board_name.clone(),
            space_id: item.space_id.clone(),
            space_key: item.space_key.clone(),
            space_name: item.space_name.clone(),
            parent_id: item.parent_id.clone(),
            parent_title: item.parent_title.clone(),
        });
    }
    out
}

/// Keeps the tools this agent is set to use and fills each one with the description the app defines for that tool.
pub(crate) fn tool_prompt_uses(list: &[agentfile::Use], enabled: &[NamedId]) -> Vec<agentdef::Use> {
    let mut uses = catalog_uses(list, enabled);
    let details: BTreeMap<String, String> = tools::catalog()
        .into_iter()
        .map(|spec| (spec.id, spec.detail))
        .collect();
    for item in uses.iter_mut() {
        if let Some(detail) = details.get(&item.id).map(|d| d.trim()).filter(|d| !d.is_empty()) {
            item.description = detail.to_string();
        }
    }
    uses
}

/// Turns a saved "atlassian" choice into the Jira and Confluence tags that Settings currently offers.
/// A tag the agent already set keeps its note.
fn spread_atlassian(list: &[agentfile::Use], names: &BTreeMap<String, String>) -> Vec<agentfile::Use> {
    let mut legacy = String::new();
    let mut found = false;
    let mut out: Vec<agentfile::Use> = Vec::with_capacity(list.len() + 1);
    let mut present: BTreeSet<String> = BTreeSet::new();
    for item in list {
        let id = item.id.trim();
        if id == MCP_ATLASSIAN {
            found = true;
            if legacy.is_empty() {
                legacy = item.description.clone();
            }
            continue;
        }
        present.insert(id.to_string());
        out.push(item.clone());
    }
    if !found {
        return list.to_vec();
    }
    for id in [MCP_JIRA, MCP_CONFLUENCE] {
        if names.contains_key(id) && !present.contains(id) {
            out.push(agentfile::Use {
                id: id.to_string(),
                description: legacy.clone(),
                ..Default::default()
            });
        }
    }
    out
}
