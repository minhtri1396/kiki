use serde_json::json;

use super::*;

#[test]
fn every_tool_has_its_own_dialect() {
    for tool in AiTool::ALL {
        let d = dialect(tool);
        assert_eq!(d.tool(), tool);
        assert!(
            d.settings_file().starts_with(&format!(".{}/", tool.id())),
            "{tool}: {}",
            d.settings_file()
        );
        assert!(!d.events().is_empty());
    }
}

#[test]
fn settings_files_and_titles() {
    let files: Vec<_> = AiTool::ALL
        .iter()
        .map(|t| (dialect(*t).title(), dialect(*t).settings_file()))
        .collect();
    assert_eq!(
        files,
        [
            ("Cursor", ".cursor/hooks.json"),
            ("Claude", ".claude/settings.json"),
            ("Codex", ".codex/hooks.json"),
            ("Gemini", ".gemini/settings.json"),
        ]
    );
}

#[test]
fn events_per_tool() {
    assert_eq!(dialect(AiTool::Cursor).events().len(), 11);
    assert_eq!(
        dialect(AiTool::Claude).events(),
        ["UserPromptSubmit", "PreToolUse", "PostToolUse", "SubagentStart"]
    );
    assert_eq!(dialect(AiTool::Codex).events(), dialect(AiTool::Claude).events());
    assert_eq!(
        dialect(AiTool::Gemini).events(),
        ["BeforeAgent", "BeforeModel", "AfterModel", "BeforeTool", "AfterTool"]
    );
}

#[test]
fn entry_shape_per_tool() {
    assert_eq!(
        dialect(AiTool::Cursor).entry("c"),
        json!({"command": "c", "failClosed": false, "timeout": 20})
    );
    let claude = json!({"hooks": [{"type": "command", "command": "c", "timeout": 20}]});
    assert_eq!(dialect(AiTool::Claude).entry("c"), claude);
    assert_eq!(dialect(AiTool::Codex).entry("c"), claude);
    assert_eq!(
        dialect(AiTool::Gemini).entry("c"),
        json!({"hooks": [{"type": "command", "command": "c", "timeout": 20000, "name": "kiki-guardrail"}]})
    );
}

#[test]
fn layouts() {
    assert_eq!(dialect(AiTool::Cursor).layout(), HookLayout::Flat);
    for tool in [AiTool::Claude, AiTool::Codex, AiTool::Gemini] {
        assert_eq!(dialect(tool).layout(), HookLayout::Grouped);
    }
}
