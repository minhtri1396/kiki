//! Text diff of one agent or repository between this app and a team folder.

use anyhow::{anyhow, Result};
use kiki_agentfile as agentfile;
use kiki_console as console;
use kiki_util::path;
use serde_json::{json, Value};

use super::shared_repo::{parse_shared_repo, SharedRepo};
use super::views::display_name;

/// An agent's diffable text and display name, when the agent exists in a folder.
pub(crate) struct AgentText {
    pub text: String,
    pub name: String,
}

/// The agent `id` in `dir`, or `None` when it is not there.
pub(crate) fn agent_file_text(dir: &str, id: &str) -> Result<Option<AgentText>> {
    let Some(file) = agentfile::find(dir, id)? else {
        return Ok(None);
    };
    let raw = std::fs::read(&file.path).map_err(|e| anyhow!("open {}: {e}", file.path))?;
    // The category is the folder, not a line in the file. Prefix it so a move still diffs.
    let text = format!("category: {}\n{}", file.category, String::from_utf8_lossy(&raw));
    Ok(Some(AgentText {
        text,
        name: display_name(&file),
    }))
}

/// The canonical text of repository `id` in `dir`, or `None` when it is not there.
pub(crate) fn repo_share_text(dir: &str, id: &str) -> Result<Option<String>> {
    let file = path::join(&[dir, &format!("{id}.yaml")]);
    let raw = match std::fs::read(&file) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow!("open {file}: {e}")),
    };
    Ok(parse_shared_repo(&raw, &format!("{id}.yaml"))?
        .as_ref()
        .map(SharedRepo::canonical))
}

/// The reply of the diff endpoint: `app` is the left side, the folder's text the right side.
pub(crate) fn diff_payload(id: &str, name: &str, folder: &str, app_text: &str, folder_text: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "folder": folder,
        "diff": console::text_diff(app_text, folder_text),
    })
}
