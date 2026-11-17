use serde_json::{json, Value};

use kiki_aitool::AiTool;
use kiki_util::json::marshal;

use super::layout::grouped_entry;
use super::{HookDialect, HookLayout};

const EVENTS: [&str; 5] = ["BeforeAgent", "BeforeModel", "AfterModel", "BeforeTool", "AfterTool"];

/// Gemini: `~/.gemini/settings.json`, grouped named entries (timeout in milliseconds), a `decision` reply.
pub(super) struct Gemini;

impl HookDialect for Gemini {
    fn tool(&self) -> AiTool {
        AiTool::Gemini
    }

    fn title(&self) -> &'static str {
        "Gemini"
    }

    fn settings_file(&self) -> &'static str {
        ".gemini/settings.json"
    }

    fn events(&self) -> &'static [&'static str] {
        &EVENTS
    }

    fn layout(&self) -> HookLayout {
        HookLayout::Grouped
    }

    fn entry(&self, command: &str) -> Value {
        grouped_entry(command, 20000, "kiki-guardrail")
    }

    fn reply_body(&self, _event: &str, allow: bool, message: &str) -> String {
        if allow {
            return "{}".to_string();
        }
        marshal(&json!({"decision": "deny", "reason": message}))
    }
}
