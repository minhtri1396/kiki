//! Typed access to the loosely shaped JSON payloads each AI tool sends.

use serde_json::{Map, Value};

pub(crate) type Obj = Map<String, Value>;

/// Objects nested inside a payload that can hold the same fields as the payload itself.
pub(super) const NESTS: [&str; 4] = ["tool_input", "toolInput", "arguments", "input"];

const PATH_KEYS: [&str; 8] = [
    "file_path",
    "filePath",
    "path",
    "target_file",
    "target_notebook",
    "notebook_path",
    "paths",
    "files",
];

/// Parses a hook payload as a JSON object; anything else is an empty object.
pub(crate) fn object(raw: &[u8]) -> Obj {
    match serde_json::from_slice::<Value>(raw) {
        Ok(Value::Object(m)) => m,
        _ => Obj::new(),
    }
}

/// The first non-blank string among `keys`, looked up in the payload and then in its nested input objects.
pub(crate) fn field(m: &Obj, keys: &[&str]) -> String {
    let s = lookup(m, keys);
    if !s.is_empty() {
        return s;
    }
    for nest in NESTS {
        let Some(Value::Object(child)) = m.get(nest) else {
            continue;
        };
        let s = lookup(child, keys);
        if !s.is_empty() {
            return s;
        }
    }
    String::new()
}

fn lookup(m: &Obj, keys: &[&str]) -> String {
    for key in keys {
        if let Some(Value::String(text)) = m.get(*key) {
            if !text.trim().is_empty() {
                return text.trim().to_string();
            }
        }
    }
    String::new()
}

pub(super) fn tool_name(m: &Obj) -> String {
    field(m, &["tool_name", "toolName"])
}

pub(super) fn command(m: &Obj) -> String {
    field(m, &["command", "cmd"])
}

pub(super) fn mcp_server(m: &Obj) -> String {
    field(
        m,
        &[
            "server",
            "server_name",
            "serverName",
            "mcp_server",
            "mcpServer",
            "mcp_server_name",
        ],
    )
}

pub(super) fn agent_name(m: &Obj) -> String {
    field(
        m,
        &[
            "subagent_type",
            "subagentType",
            "agent_type",
            "agentType",
            "agent_id",
            "agentId",
        ],
    )
}

/// Every file path the payload names, in the payload and in its nested input objects.
pub(super) fn paths_of(m: &Obj) -> Vec<String> {
    let mut out = Vec::new();
    let collect = |src: &Obj, out: &mut Vec<String>| {
        for key in PATH_KEYS {
            if let Some(v) = src.get(key) {
                out.extend(strings_from(v));
            }
        }
    };
    collect(m, &mut out);
    for nest in NESTS {
        if let Some(Value::Object(child)) = m.get(nest) {
            collect(child, &mut out);
        }
    }
    out
}

/// The trimmed non-blank strings of a string or an array of strings.
fn strings_from(value: &Value) -> Vec<String> {
    match value {
        Value::String(t) if !t.trim().is_empty() => vec![t.trim().to_string()],
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
