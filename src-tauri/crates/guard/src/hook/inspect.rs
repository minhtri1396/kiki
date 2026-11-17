//! Reads the event, working folder and absolute paths out of a hook payload and finds the KIKI that owns them.

use kiki_util::path as fpath;
use serde_json::Value;

use crate::classify::{field, object};
use crate::endpoint::Endpoint;
use crate::policy::inside;

/// Deepest nesting searched for absolute paths.
const MAX_DEPTH: usize = 6;

/// What a hook payload says about where it runs.
pub(super) struct HookInput {
    pub event: String,
    /// Absolute working folder, empty when the payload has none.
    pub cwd: String,
    /// Every absolute path (not a URL) anywhere in the payload.
    pub paths: Vec<String>,
}

pub(super) fn inspect(raw: &[u8]) -> HookInput {
    let m = object(raw);
    let event = field(&m, &["hook_event_name", "hookEventName", "event", "hook_event"]);
    let mut cwd = field(&m, &["cwd", "workspaceRoot", "working_directory"]);
    if !fpath::is_abs(&cwd) {
        cwd = workspace_root(&m);
    }
    let paths = absolute_paths(&Value::Object(m), 0);
    HookInput { event, cwd, paths }
}

/// The first absolute folder in `workspace_roots`, or empty.
fn workspace_root(m: &serde_json::Map<String, Value>) -> String {
    let Some(Value::Array(roots)) = m.get("workspace_roots") else {
        return String::new();
    };
    roots
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .find(|text| fpath::is_abs(text))
        .unwrap_or_default()
        .to_string()
}

fn absolute_paths(value: &Value, depth: usize) -> Vec<String> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    match value {
        Value::Object(map) => map.values().flat_map(|item| absolute_paths(item, depth + 1)).collect(),
        Value::Array(items) => items.iter().flat_map(|item| absolute_paths(item, depth + 1)).collect(),
        Value::String(t) => {
            let text = t.trim();
            if fpath::is_abs(text) && !text.contains("://") && text.len() < 4096 {
                vec![text.to_string()]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// The endpoint whose repositories hold the working folder, else the first one that holds any payload path.
pub(super) fn find_owner<'a>(eps: &'a [Endpoint], cwd: &str, paths: &[String]) -> Option<&'a Endpoint> {
    if !cwd.is_empty() {
        if let Some(ep) = eps.iter().find(|ep| endpoint_owns(ep, cwd)) {
            return Some(ep);
        }
    }
    eps.iter().find(|ep| !first_owned(ep, paths).is_empty())
}

fn endpoint_owns(ep: &Endpoint, path: &str) -> bool {
    ep.paths.iter().any(|root| inside(root, path))
}

/// The first path the endpoint owns, or empty.
pub(super) fn first_owned(ep: &Endpoint, paths: &[String]) -> String {
    paths
        .iter()
        .find(|path| endpoint_owns(ep, path))
        .cloned()
        .unwrap_or_default()
}
