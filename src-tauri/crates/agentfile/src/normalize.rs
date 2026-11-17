//! Validation and cleanup applied to a [`File`] before it is saved.

use std::collections::HashSet;

use anyhow::{anyhow, Result};
use kiki_agentdef as agentdef;
use kiki_aitool::AiTool;

use crate::model::{clean_handoffs, custom_guide, Collaborator, File, Repository, Use, MAX_HANDOFF_LOOPS};

/// True for ids that start with a lower case letter and use only lower case letters, digits and `_`.
pub(crate) fn is_slug(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The AI tool an agent file names, when it is one Orca can start. The match is exact.
pub(crate) fn known_tool(tool: &str) -> Option<AiTool> {
    AiTool::parse(tool)
}

/// Returns a trimmed, validated copy of `file`, or the message the editor shows for the first problem.
pub(crate) fn normalize_file(file: &File) -> Result<File> {
    let mut file = file.clone();
    file.id = file.id.trim().to_string();
    file.name = file.name.trim().to_string();
    file.description = file.description.trim().to_string();
    file.category = file.category.trim().to_lowercase();
    file.ai_tool = file.ai_tool.trim().to_string();
    file.model = file.model.trim().to_string();
    file.prompt = file.prompt.replace("\r\n", "\n");
    if !is_slug(&file.id) {
        return Err(anyhow!(
            "enter an agent id that starts with a letter and uses only letters, digits, and underscores"
        ));
    }
    if !is_slug(&file.category) {
        return Err(anyhow!(
            "enter a category folder name that starts with a letter and uses only letters, digits, and underscores"
        ));
    }
    if file.name.is_empty() || file.name.contains(['\r', '\n']) {
        return Err(anyhow!("enter a name"));
    }
    file.works_with = normalize_peers(&file.works_with, &file.id)?;
    file.mcps = normalize_uses(&file.mcps, "MCP servers")?;
    file.tools = normalize_uses(&file.tools, "tools")?;
    if known_tool(&file.ai_tool).is_none() {
        return Err(anyhow!("choose an AI tool"));
    }
    if file.model.is_empty() || file.model.contains(['\r', '\n']) {
        return Err(anyhow!("choose a model"));
    }
    file.repos = normalize_repos(&file.repos)?;
    Ok(file)
}

/// Drops blank, duplicate and self references, and checks each remaining collaborator's messages.
fn normalize_peers(peers: &[Collaborator], own_id: &str) -> Result<Vec<Collaborator>> {
    let mut with: Vec<Collaborator> = Vec::with_capacity(peers.len());
    let mut seen_peer: HashSet<String> = HashSet::new();
    for peer in peers {
        let id = peer.id.trim();
        if id.is_empty() || id == own_id || seen_peer.contains(id) {
            continue;
        }
        if !is_slug(id) {
            return Err(anyhow!("works with must be an agent id"));
        }
        seen_peer.insert(id.to_string());
        let handoffs = clean_handoffs(&peer.handoffs);
        for handoff in &handoffs {
            if handoff.send.is_empty() {
                return Err(anyhow!("enter what to send to {id}, or remove the condition"));
            }
            if handoff.max_loops < 0 || handoff.max_loops > MAX_HANDOFF_LOOPS {
                return Err(anyhow!(
                    "enter a loop limit from 1 to {MAX_HANDOFF_LOOPS} for the message to {id}"
                ));
            }
        }
        with.push(Collaborator {
            id: id.to_string(),
            handoffs,
        });
    }
    Ok(with)
}

/// Drops blank and duplicate repositories, and checks names and guide files.
fn normalize_repos(list: &[Repository]) -> Result<Vec<Repository>> {
    let mut repos: Vec<Repository> = Vec::with_capacity(list.len());
    let mut seen: HashSet<String> = HashSet::new();
    for repo in list {
        let kind = repo.kind.trim();
        if kind.is_empty() || seen.contains(kind) {
            continue;
        }
        if !is_slug(kind) {
            return Err(anyhow!("repos must use folder-safe names"));
        }
        seen.insert(kind.to_string());
        if !agentdef::valid_guide(&repo.guide) {
            return Err(anyhow!(
                "enter the guide file for {kind} as a markdown file name such as {}",
                agentdef::DEFAULT_GUIDE
            ));
        }
        repos.push(Repository {
            kind: kind.to_string(),
            guide: custom_guide(&repo.guide),
            description: repo.description.trim().to_string(),
            readonly: repo.readonly,
        });
    }
    if repos.is_empty() {
        return Err(anyhow!("choose at least one repo"));
    }
    Ok(repos)
}

fn normalize_uses(list: &[Use], label: &str) -> Result<Vec<Use>> {
    let mut out: Vec<Use> = Vec::with_capacity(list.len());
    let mut seen: HashSet<String> = HashSet::new();
    for item in list {
        let id = item.id.trim();
        if id.is_empty() || seen.contains(id) {
            continue;
        }
        if !is_slug(id) {
            return Err(anyhow!(
                "{label} must use ids that start with a letter and use only letters, digits, and underscores"
            ));
        }
        seen.insert(id.to_string());
        out.push(Use {
            id: id.to_string(),
            description: item.description.trim().to_string(),
            board_id: item.board_id.trim().to_string(),
            board_name: item.board_name.trim().to_string(),
            space_id: item.space_id.trim().to_string(),
            space_key: item.space_key.trim().to_string(),
            space_name: item.space_name.trim().to_string(),
            parent_id: item.parent_id.trim().to_string(),
            parent_title: item.parent_title.trim().to_string(),
        });
    }
    Ok(out)
}
