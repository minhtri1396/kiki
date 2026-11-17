use serde_json::{json, Value};

use kiki_aitool::AiTool;
use kiki_util::json::marshal;

use super::layout::grouped_entry;
use super::{HookDialect, HookLayout};

const EVENTS: [&str; 4] = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "SubagentStart"];

/// Claude: `~/.claude/settings.json`, grouped entries, a `hookSpecificOutput` reply on tool events.
pub(super) struct Claude;

impl HookDialect for Claude {
    fn tool(&self) -> AiTool {
        AiTool::Claude
    }

    fn title(&self) -> &'static str {
        "Claude"
    }

    fn settings_file(&self) -> &'static str {
        ".claude/settings.json"
    }

    fn events(&self) -> &'static [&'static str] {
        &EVENTS
    }

    fn layout(&self) -> HookLayout {
        HookLayout::Grouped
    }

    fn entry(&self, command: &str) -> Value {
        grouped_entry(command, 20, "")
    }

    fn reply_body(&self, event: &str, allow: bool, message: &str) -> String {
        hook_specific_reply(event, allow, message)
    }
}

/// The reply format Claude and Codex share: tool permission events carry a `hookSpecificOutput`,
/// every other event is a bare `{}` or a block decision.
pub(super) fn hook_specific_reply(event: &str, allow: bool, message: &str) -> String {
    match event.trim().to_lowercase().as_str() {
        "pretooluse" | "permissionrequest" => {
            let name = if event.eq_ignore_ascii_case("PermissionRequest") {
                "PermissionRequest"
            } else {
                "PreToolUse"
            };
            let out: Value = if allow {
                json!({"hookEventName": name, "permissionDecision": "allow"})
            } else {
                json!({
                    "hookEventName": name,
                    "permissionDecision": "deny",
                    "permissionDecisionReason": message,
                })
            };
            marshal(&json!({"hookSpecificOutput": out}))
        }
        _ if allow => "{}".to_string(),
        _ => marshal(&json!({"decision": "block", "reason": message})),
    }
}
