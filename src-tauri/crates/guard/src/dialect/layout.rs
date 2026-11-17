//! The two shapes hook entries take inside a settings file, and how to find or drop ours in each.

use anyhow::bail;
use serde_json::{json, Map, Value};

/// Text our hook command always contains, between the executable and the AI tool id.
pub(crate) const MARKER: &str = " guardrail ";

/// How an event's list is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookLayout {
    /// `"event": [{"command": ...}]` (Cursor).
    Flat,
    /// `"event": [{"hooks": [{"type": "command", "command": ...}]}]` (Claude, Codex, Gemini).
    Grouped,
}

impl HookLayout {
    /// The event's entries without ours. Entries that are not objects, or hold other tools' hooks, are kept.
    pub(crate) fn drop_ours(self, value: &Value) -> anyhow::Result<Vec<Value>> {
        let list = match value {
            Value::Null => return Ok(Vec::new()),
            Value::Array(list) => list,
            _ => bail!("hooks are not a list"),
        };
        Ok(match self {
            HookLayout::Flat => list
                .iter()
                .filter(|item| !item.is_object() || !is_ours(item))
                .cloned()
                .collect(),
            HookLayout::Grouped => list.iter().filter_map(drop_ours_from_group).collect(),
        })
    }

    /// True when the event's list has a hook whose command contains `suffix`.
    pub(crate) fn has_command(self, value: Option<&Value>, suffix: &str) -> bool {
        let Some(Value::Array(list)) = value else {
            return false;
        };
        match self {
            HookLayout::Flat => list.iter().any(|item| command_of(item).contains(suffix)),
            HookLayout::Grouped => list.iter().any(|item| match item.get("hooks") {
                Some(Value::Array(inner)) => inner.iter().any(|hook| command_of(hook).contains(suffix)),
                _ => false,
            }),
        }
    }
}

/// A grouped entry for one event: one command hook with the given timeout and, when not empty, display name.
pub(crate) fn grouped_entry(command: &str, timeout: u32, name: &str) -> Value {
    let mut hook = Map::new();
    hook.insert("type".to_string(), json!("command"));
    hook.insert("command".to_string(), json!(command));
    hook.insert("timeout".to_string(), json!(timeout));
    if !name.is_empty() {
        hook.insert("name".to_string(), json!(name));
    }
    json!({"hooks": [Value::Object(hook)]})
}

fn command_of(obj: &Value) -> &str {
    obj.get("command").and_then(Value::as_str).unwrap_or("")
}

fn is_ours(row: &Value) -> bool {
    command_of(row).contains(MARKER)
}

/// A group without our hooks; `None` when nothing is left in it.
fn drop_ours_from_group(item: &Value) -> Option<Value> {
    let Value::Object(obj) = item else {
        return Some(item.clone());
    };
    let Some(Value::Array(inner)) = obj.get("hooks") else {
        return Some(item.clone());
    };
    let kept: Vec<Value> = inner
        .iter()
        .filter(|hook| !(hook.is_object() && is_ours(hook)))
        .cloned()
        .collect();
    if kept.is_empty() {
        return None;
    }
    let mut obj = obj.clone();
    obj.insert("hooks".to_string(), Value::Array(kept));
    Some(Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ours() -> String {
        "\"/k\" guardrail cursor".to_string()
    }

    #[test]
    fn flat_drops_only_our_objects() {
        let list = json!([{"command": ours()}, {"command": "echo keep"}, "text", 3]);
        let got = HookLayout::Flat.drop_ours(&list).unwrap();
        assert_eq!(got, vec![json!({"command": "echo keep"}), json!("text"), json!(3)]);
    }

    #[test]
    fn grouped_drops_our_hooks_and_empty_groups() {
        let list = json!([
            {"hooks": [{"command": ours()}]},
            {"hooks": [{"command": ours()}, {"command": "echo keep"}], "matcher": "x"},
            {"other": 1},
            "text"
        ]);
        let got = HookLayout::Grouped.drop_ours(&list).unwrap();
        assert_eq!(
            got,
            vec![
                json!({"hooks": [{"command": "echo keep"}], "matcher": "x"}),
                json!({"other": 1}),
                json!("text")
            ]
        );
    }

    #[test]
    fn null_is_empty_and_other_values_are_not_lists() {
        assert!(HookLayout::Flat.drop_ours(&Value::Null).unwrap().is_empty());
        let err = HookLayout::Grouped.drop_ours(&json!({})).unwrap_err();
        assert_eq!(err.to_string(), "hooks are not a list");
    }

    #[test]
    fn has_command_looks_in_the_layout_of_each_dialect() {
        let flat = json!([{"command": ours()}]);
        let grouped = json!([{"hooks": [{"command": ours()}]}]);
        assert!(HookLayout::Flat.has_command(Some(&flat), "guardrail cursor"));
        assert!(!HookLayout::Grouped.has_command(Some(&flat), "guardrail cursor"));
        assert!(HookLayout::Grouped.has_command(Some(&grouped), "guardrail cursor"));
        assert!(!HookLayout::Flat.has_command(None, "guardrail cursor"));
    }

    #[test]
    fn grouped_entry_names_the_hook_only_when_asked() {
        let named = grouped_entry("c", 5, "n");
        assert_eq!(named["hooks"][0]["name"], "n");
        assert!(grouped_entry("c", 5, "")["hooks"][0].get("name").is_none());
    }
}
