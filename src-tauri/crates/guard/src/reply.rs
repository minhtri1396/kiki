//! The JSON an AI tool hook prints back to the tool.

use kiki_aitool::AiTool;

use crate::dialect::dialect;

const DEFAULT_BLOCK_MESSAGE: &str = "Guardrail: KIKI blocked this action.";

/// The reply for a hook `event`. The shape matches the tool's hook format; the text ends with a newline.
/// A block without a message gets a default one.
pub fn reply(tool: AiTool, event: &str, allow: bool, message: &str) -> String {
    let message = if message.trim().is_empty() && !allow {
        DEFAULT_BLOCK_MESSAGE
    } else {
        message
    };
    dialect(tool).reply_body(event, allow, message) + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_uses_the_tool_hook_shape() {
        let cursor = reply(AiTool::Cursor, "beforeMCPExecution", false, "blocked");
        assert!(
            cursor.contains(r#""permission":"deny""#) && cursor.contains("blocked"),
            "cursor = {cursor}"
        );
        let claude = reply(AiTool::Claude, "PreToolUse", false, "blocked");
        assert!(claude.contains(r#""permissionDecision":"deny""#), "claude = {claude}");
        let prompt = reply(AiTool::Codex, "UserPromptSubmit", true, "");
        assert_eq!(prompt.trim(), "{}", "prompt = {prompt}");
        let gemini = reply(AiTool::Gemini, "BeforeTool", false, "blocked");
        assert!(gemini.contains(r#""decision":"deny""#), "gemini = {gemini}");
    }

    #[test]
    fn every_reply_is_one_line_ending_in_a_newline() {
        for tool in AiTool::ALL {
            for allow in [true, false] {
                let text = reply(tool, "PreToolUse", allow, "<m>");
                assert!(
                    text.ends_with('\n') && text.matches('\n').count() == 1,
                    "{tool} = {text:?}"
                );
            }
        }
    }

    #[test]
    fn exact_reply_text_per_tool() {
        let cases = [
            (AiTool::Cursor, true, "{\"permission\":\"allow\"}\n"),
            (
                AiTool::Cursor,
                false,
                "{\"agent_message\":\"\\u003cm\\u003e\",\"permission\":\"deny\",\"user_message\":\"\\u003cm\\u003e\"}\n",
            ),
            (AiTool::Claude, true, "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\"}}\n"),
            (
                AiTool::Codex,
                false,
                "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"\\u003cm\\u003e\"}}\n",
            ),
            (AiTool::Gemini, true, "{}\n"),
            (AiTool::Gemini, false, "{\"decision\":\"deny\",\"reason\":\"\\u003cm\\u003e\"}\n"),
        ];
        for (tool, allow, want) in cases {
            assert_eq!(reply(tool, "PreToolUse", allow, "<m>"), want, "{tool} allow={allow}");
        }
    }

    #[test]
    fn claude_and_codex_answer_other_events_with_a_block_decision() {
        for tool in [AiTool::Claude, AiTool::Codex] {
            assert_eq!(
                reply(tool, "Stop", false, "no"),
                "{\"decision\":\"block\",\"reason\":\"no\"}\n"
            );
            assert_eq!(
                reply(tool, "permissionrequest", true, ""),
                "{\"hookSpecificOutput\":{\"hookEventName\":\"PermissionRequest\",\"permissionDecision\":\"allow\"}}\n"
            );
        }
    }

    #[test]
    fn a_block_without_a_message_gets_the_default_text() {
        for tool in AiTool::ALL {
            let text = reply(tool, "PreToolUse", false, "  ");
            assert!(text.contains("Guardrail: KIKI blocked this action."), "{tool} = {text}");
        }
    }
}
