//! Agent files as the web UI sees them (JSON rows) and as the prompt builders see them (`agentdef` and `console`
//! values).

use serde_json::{json, Map, Value};

use kiki_agentdef as agentdef;
use kiki_agentfile as agentfile;
use kiki_console as console;

/// One agent for `GET /api/agents/{id}`.
pub(crate) fn agent_json(file: &agentfile::File) -> Value {
    json!({
        "id": file.id,
        "name": file.name,
        "description": file.description,
        "category": file.category,
        "enabled": file.enabled,
        "tool": file.ai_tool,
        "model": file.model,
        "repos": repository_json(&file.repos),
        "worksWith": teamwork_json(&file.works_with),
        "mcps": use_json(&file.mcps),
        "tools": use_json(&file.tools),
        "prompt": file.prompt,
    })
}

/// One agent row of the `/api/state` document.
pub(crate) fn agent_row(file: &agentfile::File) -> Value {
    json!({
        "id": file.id,
        "title": file.name,
        "group": file.category,
        "category": file.category,
        "repos": repo_kinds(&file.repos),
        "repositoryUses": repository_json(&file.repos),
        "worksWith": peer_ids(&file.works_with),
        "teamwork": teamwork_json(&file.works_with),
        "mcps": use_json(&file.mcps),
        "tools": use_json(&file.tools),
        "summary": file.description,
        "enabled": file.enabled,
        "tool": file.ai_tool,
        "model": file.model,
    })
}

fn repo_kinds(list: &[agentfile::Repository]) -> Vec<String> {
    list.iter()
        .filter(|repo| !repo.kind.is_empty())
        .map(|repo| repo.kind.clone())
        .collect()
}

fn peer_ids(list: &[agentfile::Collaborator]) -> Vec<String> {
    list.iter()
        .filter(|peer| !peer.id.is_empty())
        .map(|peer| peer.id.clone())
        .collect()
}

fn teamwork_json(list: &[agentfile::Collaborator]) -> Vec<Value> {
    list.iter()
        .filter(|peer| !peer.id.is_empty())
        .map(|peer| {
            let handoffs: Vec<Value> = peer
                .handoffs
                .iter()
                .map(|pair| {
                    json!({ "when": pair.when, "send": pair.send, "reply": pair.reply, "maxLoops": pair.max_loops })
                })
                .collect();
            json!({ "id": peer.id, "handoffs": handoffs })
        })
        .collect()
}

/// Adds `key` to `row` unless the trimmed value is empty.
fn put_use(row: &mut Map<String, Value>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        row.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn use_json(list: &[agentfile::Use]) -> Vec<Value> {
    list.iter()
        .filter(|item| !item.id.is_empty())
        .map(|item| {
            let mut row = Map::new();
            row.insert("id".into(), Value::String(item.id.clone()));
            row.insert("description".into(), Value::String(item.description.clone()));
            put_use(&mut row, "boardId", &item.board_id);
            put_use(&mut row, "boardName", &item.board_name);
            put_use(&mut row, "spaceId", &item.space_id);
            put_use(&mut row, "spaceKey", &item.space_key);
            put_use(&mut row, "spaceName", &item.space_name);
            put_use(&mut row, "parentId", &item.parent_id);
            put_use(&mut row, "parentTitle", &item.parent_title);
            Value::Object(row)
        })
        .collect()
}

fn repository_json(list: &[agentfile::Repository]) -> Vec<Value> {
    list.iter()
        .filter(|repo| !repo.kind.is_empty())
        .map(|repo| {
            json!({
                "kind": repo.kind,
                "guide": agentdef::guide_file(&repo.guide),
                "description": repo.description,
                "readonly": repo.readonly,
            })
        })
        .collect()
}

pub(crate) fn catalog_repos(list: &[agentfile::Repository]) -> Vec<agentdef::Repo> {
    list.iter()
        .map(|repo| agentdef::Repo {
            kind: repo.kind.clone(),
            guide: repo.guide.clone(),
            description: repo.description.clone(),
            readonly: repo.readonly,
        })
        .collect()
}

/// The peers an agent works with, each with the notes it sends them.
pub(crate) fn catalog_peers(list: &[agentfile::Collaborator]) -> Vec<agentdef::Peer> {
    list.iter()
        .map(|peer| {
            let mut note = String::new();
            for pair in &peer.handoffs {
                note.push_str(&pair.send);
                note.push('\n');
            }
            agentdef::Peer {
                id: peer.id.clone(),
                collaboration: note.trim().to_string(),
            }
        })
        .collect()
}

pub(crate) fn launch_peers(list: &[agentfile::Collaborator]) -> Vec<console::Peer> {
    list.iter()
        .map(|peer| console::Peer {
            id: peer.id.clone(),
            handoffs: peer
                .handoffs
                .iter()
                .map(|pair| console::Handoff {
                    when: pair.when.clone(),
                    send: pair.send.clone(),
                    reply: pair.reply.clone(),
                    max_loops: pair.max_loops,
                })
                .collect(),
        })
        .collect()
}
