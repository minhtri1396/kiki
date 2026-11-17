use serde_json::{json, Value};

use kiki_aitool::AiTool;
use kiki_util::json::marshal;

use super::{HookDialect, HookLayout};
use crate::settings::Root;

const EVENTS: [&str; 11] = [
    "beforeSubmitPrompt",
    "preToolUse",
    "postToolUse",
    "beforeShellExecution",
    "afterShellExecution",
    "beforeMCPExecution",
    "afterMCPExecution",
    "beforeReadFile",
    "afterFileEdit",
    "subagentStart",
    "afterAgentResponse",
];

/// Cursor: `~/.cursor/hooks.json`, flat entries, a `permission` reply.
pub(super) struct Cursor;

impl HookDialect for Cursor {
    fn tool(&self) -> AiTool {
        AiTool::Cursor
    }

    fn title(&self) -> &'static str {
        "Cursor"
    }

    fn settings_file(&self) -> &'static str {
        ".cursor/hooks.json"
    }

    fn events(&self) -> &'static [&'static str] {
        &EVENTS
    }

    fn layout(&self) -> HookLayout {
        HookLayout::Flat
    }

    fn entry(&self, command: &str) -> Value {
        json!({
            "command": command,
            "failClosed": false,
            "timeout": 20,
        })
    }

    fn prepare_root(&self, root: &mut Root) {
        if !root.contains_key("version") {
            root.insert("version".to_string(), json!(1));
        }
    }

    fn reply_body(&self, _event: &str, allow: bool, message: &str) -> String {
        let body = if allow {
            json!({"permission": "allow"})
        } else {
            json!({"permission": "deny", "user_message": message, "agent_message": message})
        };
        marshal(&body)
    }
}
