//! Shrinks a hook payload to the fields the guardrail decides on.

use kiki_util::json::marshal;
use serde_json::{Map, Value};

use crate::classify::object;

/// Longest text kept for a field, in bytes.
const MAX_TEXT_BYTES: usize = 8000;

/// Fields copied from the payload and from its nested input objects.
const KEPT_FIELDS: &[&str] = &[
    "hook_event_name",
    "hookEventName",
    "event",
    "hook_event",
    "cwd",
    "workspaceRoot",
    "working_directory",
    "tool_name",
    "toolName",
    "command",
    "cmd",
    "server",
    "server_name",
    "serverName",
    "mcp_server",
    "mcpServer",
    "mcp_server_name",
    "subagent_type",
    "subagentType",
    "agent_type",
    "agentType",
    "agent_id",
    "agentId",
    "file_path",
    "filePath",
    "path",
    "target_file",
    "target_notebook",
    "notebook_path",
    "paths",
    "files",
];

/// Keeps the fields the guardrail decides on and drops file contents and prompts.
pub fn reduce(raw: &[u8]) -> Vec<u8> {
    let m = object(raw);
    let mut kept = Map::new();
    copy_fields(&mut kept, &m);
    for nest in ["tool_input", "toolInput", "arguments", "input"] {
        let Some(Value::Object(child)) = m.get(nest) else {
            continue;
        };
        let mut inner = Map::new();
        copy_fields(&mut inner, child);
        if !inner.is_empty() {
            kept.insert(nest.to_string(), Value::Object(inner));
        }
    }
    if let Some(roots @ Value::Array(_)) = m.get("workspace_roots") {
        kept.insert("workspace_roots".to_string(), roots.clone());
    }
    marshal(&Value::Object(kept)).into_bytes()
}

fn copy_fields(dst: &mut Map<String, Value>, src: &Map<String, Value>) {
    for key in KEPT_FIELDS {
        let Some(value) = src.get(*key) else {
            continue;
        };
        let value = match value {
            Value::String(text) if text.len() > MAX_TEXT_BYTES => Value::String(truncate_bytes(text, MAX_TEXT_BYTES)),
            other => other.clone(),
        };
        dst.insert(key.to_string(), value);
    }
}

/// The first `n` bytes of `text`. A cut inside a multi-byte character turns each broken byte into U+FFFD,
/// as earlier versions did, so the text sent to the app stays the same.
fn truncate_bytes(text: &str, n: usize) -> String {
    let mut bytes = &text.as_bytes()[..n];
    let mut out = String::new();
    loop {
        match std::str::from_utf8(bytes) {
            Ok(s) => {
                out.push_str(s);
                return out;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                out.push_str(std::str::from_utf8(&bytes[..valid]).unwrap_or(""));
                out.push('\u{fffd}');
                bytes = &bytes[valid + 1..];
            }
        }
    }
}
