//! The embedded agent catalog written into a new agent folder, and the migration of old flat files into
//! category folders.

use std::fs;

use anyhow::{anyhow, Result};
use kiki_agentdef as agentdef;
use kiki_aitool::AiTool;
use kiki_console as console;
use kiki_util::path::{is_abs, join};

use crate::disk::{has_agent_markdown, mkdir_all, move_file, not_exist, path_err};
use crate::model::{clean_handoffs, custom_guide, Collaborator, Handoff, Repository, Use};
use crate::normalize::known_tool;
use crate::parse::Header;
use crate::serialize;

/// Creates the agent folder when it is missing. The first time that
/// folder has no agent markdown, it writes one file per embedded agent inside
/// a category folder. A later run leaves those files as they are, and moves a
/// top-level `<id>.md` into its category folder.
pub fn ensure(dir: &str, default_tool: &str) -> Result<()> {
    let dir = dir.trim();
    if dir.is_empty() || !is_abs(dir) {
        return Err(anyhow!("agent path is not set"));
    }
    mkdir_all(dir)?;
    if !has_agent_markdown(dir)? {
        let (tool, model) = default_tool_model(default_tool);
        for agent in agentdef::CATALOG.iter() {
            write_new(&category_path(dir, agent), agent, &tool, &model)?;
        }
        return Ok(());
    }
    for agent in agentdef::CATALOG.iter() {
        adopt_flat_file(dir, agent)?;
    }
    Ok(())
}

fn category_name(group: &str) -> String {
    group.trim().to_lowercase()
}

/// `<dir>/<category>/<id>.md` for an embedded agent.
fn category_path(dir: &str, agent: &agentdef::Agent) -> String {
    join(&[dir, &category_name(&agent.group), &format!("{}.md", agent.id)])
}

/// Moves `<dir>/<id>.md` into the agent's category folder unless a file is already there.
fn adopt_flat_file(dir: &str, agent: &agentdef::Agent) -> Result<()> {
    let dest = category_path(dir, agent);
    match fs::metadata(&dest) {
        Ok(_) => return Ok(()),
        Err(e) if !not_exist(&e) => return Err(path_err("stat", &dest, &e)),
        Err(_) => {}
    }
    let legacy = join(&[dir, &format!("{}.md", agent.id)]);
    match fs::metadata(&legacy) {
        Ok(_) => {}
        Err(e) if not_exist(&e) => return Ok(()),
        Err(e) => return Err(path_err("stat", &legacy, &e)),
    }
    move_file(&legacy, &dest)
}

/// The tool and model new agent files start with; an unknown or empty tool falls back to `cursor`.
fn default_tool_model(default_tool: &str) -> (String, String) {
    let tool = known_tool(default_tool.trim()).unwrap_or(AiTool::Cursor);
    let model = console::resolve_model(&console::model_catalog(), tool, "").unwrap_or_default();
    (tool.id().to_string(), model)
}

/// Writes one embedded agent as a markdown file.
fn write_new(path: &str, agent: &agentdef::Agent, tool: &str, model: &str) -> Result<()> {
    let body = agent.body()?;
    let meta = Header {
        id: agent.id.clone(),
        name: agent.title.clone(),
        description: agent.summary.clone(),
        enabled: Some(true),
        ai_tool: tool.to_string(),
        model: model.to_string(),
        repos: catalog_repos(agent),
        works_with: catalog_peers(agent),
        mcps: catalog_mcps(agent),
        ..Default::default()
    };
    crate::disk::write_atomic(path, &serialize::format(&meta, &body))
}

fn catalog_peers(agent: &agentdef::Agent) -> Vec<Collaborator> {
    let mut with = Vec::new();
    for peer in &agent.peers {
        let id = peer.id.trim();
        if id.is_empty() {
            continue;
        }
        with.push(Collaborator {
            id: id.to_string(),
            handoffs: clean_handoffs(&[Handoff {
                when: format!("This agent has work for {id}."),
                send: peer.collaboration.clone(),
                ..Default::default()
            }]),
        });
    }
    with
}

fn catalog_repos(agent: &agentdef::Agent) -> Vec<Repository> {
    let mut repos = Vec::with_capacity(agent.repos.len());
    for repo in &agent.repos {
        let kind = repo.kind.trim();
        if kind.is_empty() {
            continue;
        }
        repos.push(Repository {
            kind: kind.to_string(),
            guide: custom_guide(&repo.guide),
            description: repo.description.trim().to_string(),
            readonly: false,
        });
    }
    repos
}

fn catalog_mcps(agent: &agentdef::Agent) -> Vec<Use> {
    let mut mcps = Vec::with_capacity(agent.mcps.len());
    for item in &agent.mcps {
        let id = item.id.trim();
        if id.is_empty() {
            continue;
        }
        mcps.push(Use {
            id: id.to_string(),
            description: item.description.trim().to_string(),
            ..Default::default()
        });
    }
    mcps
}
