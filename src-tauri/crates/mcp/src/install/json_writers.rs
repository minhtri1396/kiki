//! Cursor, Gemini, and Claude keep MCP servers under `mcpServers` in a JSON settings file. The file is read as an
//! ordered object of raw values, so every other key keeps its position and its exact text.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use kiki_util::{json, path};
use serde_json::{json, Value};

use super::writer::{InstallRequest, McpWriter};
use crate::json_object::{is_null, parse_json_object, JsonObject};

/// Cursor and Gemini: one settings file per scope, servers under `mcpServers`.
pub(super) struct PlainJson {
    label: &'static str,
    folder: &'static str,
    file: &'static str,
}

pub(super) static CURSOR: PlainJson = PlainJson {
    label: "Cursor",
    folder: ".cursor",
    file: "mcp.json",
};
pub(super) static GEMINI: PlainJson = PlainJson {
    label: "Gemini",
    folder: ".gemini",
    file: "settings.json",
};

impl McpWriter for PlainJson {
    fn label(&self) -> &'static str {
        self.label
    }

    fn user_config(&self, home: &Path) -> PathBuf {
        home.join(self.folder).join(self.file)
    }

    fn project_config(&self, worktree: &Path) -> PathBuf {
        worktree.join(self.folder).join(self.file)
    }

    fn project_file_has_key(&self, data: &[u8], key: &str) -> bool {
        json_has_key(data, key)
    }

    fn user_file_has_key(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<bool> {
        servers_have_key(&read_object(data)?, request.key)
    }

    fn merge(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<Vec<u8>> {
        merge_server(self.label, data, request, false)
    }
}

/// Claude: user settings live in `~/.claude.json`, where a launch folder can also have its own entry
/// under `projects`; the launch folder's shareable file is `.mcp.json`.
pub(super) struct ClaudeJson;

pub(super) static CLAUDE: ClaudeJson = ClaudeJson;

impl McpWriter for ClaudeJson {
    fn label(&self) -> &'static str {
        "Claude"
    }

    fn user_config(&self, home: &Path) -> PathBuf {
        home.join(".claude.json")
    }

    fn project_config(&self, worktree: &Path) -> PathBuf {
        worktree.join(".mcp.json")
    }

    fn project_file_has_key(&self, data: &[u8], key: &str) -> bool {
        json_has_key(data, key)
    }

    fn user_file_has_key(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<bool> {
        claude_has(&read_object(data)?, request.worktree, request.key)
    }

    fn merge(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<Vec<u8>> {
        merge_server("Claude", data, request, true)
    }
}

/// A missing or blank file reads as an empty object.
fn read_object(data: &[u8]) -> Result<JsonObject> {
    if String::from_utf8_lossy(data).trim().is_empty() {
        return Ok(JsonObject::new());
    }
    parse_json_object(data)
}

fn json_has_key(data: &[u8], key: &str) -> bool {
    let Ok(obj) = parse_json_object(data) else {
        return false;
    };
    matches!(servers_have_key(&obj, key), Ok(true))
}

/// Adds the server to `mcpServers` and renders the whole file. Claude's entry also says `"type": "stdio"`.
fn merge_server(label: &str, data: &[u8], request: &InstallRequest<'_>, stdio_type: bool) -> Result<Vec<u8>> {
    let mut obj = read_object(data).map_err(|e| anyhow!("{label} MCP settings: {e}"))?;
    let mut servers = JsonObject::new();
    if let Some(raw) = obj.vals.get("mcpServers") {
        if !is_null(raw) {
            servers = parse_json_object(raw.as_bytes())
                .map_err(|_| anyhow!("{label} MCP settings: server list is not a JSON object"))?;
        }
    }
    servers.set(request.key, &server_document(request, stdio_type));
    obj.set("mcpServers", &servers.compact());
    Ok(obj.pretty().into_bytes())
}

fn server_document(request: &InstallRequest<'_>, stdio_type: bool) -> String {
    let mut body = json!({
        "command": "uvx",
        "args": ["mcp-atlassian"],
        "env": request.env,
    });
    if stdio_type {
        body["type"] = Value::String("stdio".into());
    }
    json::marshal(&body)
}

fn servers_have_key(obj: &JsonObject, key: &str) -> Result<bool> {
    let Some(raw) = obj.vals.get("mcpServers") else {
        return Ok(false);
    };
    if is_null(raw) {
        return Ok(false);
    }
    let servers = parse_json_object(raw.as_bytes()).map_err(|_| anyhow!("server list is not a JSON object"))?;
    Ok(servers.has(key))
}

/// Claude has the key in its global servers or in the entry for this launch folder.
fn claude_has(obj: &JsonObject, worktree: &str, key: &str) -> Result<bool> {
    if servers_have_key(obj, key)? {
        return Ok(true);
    }
    let Some(raw) = obj.vals.get("projects") else {
        return Ok(false);
    };
    if is_null(raw) || worktree.trim().is_empty() {
        return Ok(false);
    }
    let Ok(projects) = parse_json_object(raw.as_bytes()) else {
        return Ok(false);
    };
    let want = path::clean(worktree);
    for project_key in &projects.keys {
        if path::clean(project_key) != want {
            continue;
        }
        let Ok(project) = parse_json_object(projects.vals[project_key].as_bytes()) else {
            return Ok(false);
        };
        return servers_have_key(&project, key);
    }
    Ok(false)
}
