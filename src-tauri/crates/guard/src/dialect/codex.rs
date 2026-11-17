use serde_json::Value;

use kiki_aitool::AiTool;

use super::claude::hook_specific_reply;
use super::layout::grouped_entry;
use super::{HookDialect, HookLayout};

const EVENTS: [&str; 4] = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "SubagentStart"];

/// Codex: `~/.codex/hooks.json`, grouped entries, the same reply format as Claude.
pub(super) struct Codex;

impl HookDialect for Codex {
    fn tool(&self) -> AiTool {
        AiTool::Codex
    }

    fn title(&self) -> &'static str {
        "Codex"
    }

    fn settings_file(&self) -> &'static str {
        ".codex/hooks.json"
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
