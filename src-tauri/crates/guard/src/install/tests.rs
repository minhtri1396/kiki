use std::fs;
use std::path::Path;

use kiki_aitool::AiTool;

use super::*;
use crate::dialect::dialect;
use crate::testutil::{join, p, tmp};

#[test]
fn install_keeps_existing_hooks() {
    let home = tmp();
    let home = p(&home);
    let cursor = join(&home, ".cursor/hooks.json");
    fs::create_dir_all(Path::new(&cursor).parent().unwrap()).unwrap();
    let original = "{\n  \"version\": 1,\n  \"hooks\": {\n    \"beforeShellExecution\": [\n      {\"command\": \"echo keep\"}\n    ]\n  }\n}\n";
    fs::write(&cursor, original).unwrap();
    let gemini = join(&home, ".gemini/settings.json");
    fs::create_dir_all(Path::new(&gemini).parent().unwrap()).unwrap();
    fs::write(&gemini, "{\n  \"mcpServers\": {\"keep\": {\"command\": \"uvx\"}}\n}\n").unwrap();
    let broken = join(&home, ".codex/hooks.json");
    fs::create_dir_all(Path::new(&broken).parent().unwrap()).unwrap();
    fs::write(&broken, "{").unwrap();
    let err = install(&home, "/Applications/KIKI").expect_err("install should fail for broken codex");
    assert!(format!("{err:#}").contains("Codex"), "install = {err:#}");
    let text = fs::read_to_string(&cursor).unwrap();
    assert!(
        text.contains("echo keep") && text.contains("guardrail cursor"),
        "cursor hooks = {text}"
    );
    fs::read(&broken).unwrap();
    let same = fs::read_to_string(&broken).unwrap();
    assert_eq!(same, "{", "broken file changed: {same}");
    fs::remove_file(&broken).unwrap();
    install(&home, "/Applications/KIKI").unwrap();
    install(&home, "/Applications/KIKI").unwrap();
    let text = fs::read_to_string(&cursor).unwrap();
    assert_eq!(
        text.matches("guardrail cursor").count(),
        dialect(AiTool::Cursor).events().len(),
        "cursor count\n{text}"
    );
    let settings = fs::read_to_string(&gemini).unwrap();
    assert!(
        settings.contains("\"keep\"")
            && settings.matches("guardrail gemini").count() == dialect(AiTool::Gemini).events().len(),
        "gemini = {settings}"
    );
    // Claude and Codex have their own event lists.
    assert_eq!(
        dialect(AiTool::Claude).events().len(),
        dialect(AiTool::Codex).events().len()
    );
    for tool in AiTool::ALL {
        hooks_ready(&home, tool).unwrap_or_else(|e| panic!("{tool}: {e:#}"));
    }
    remove(&home).unwrap();
    let text = fs::read_to_string(&cursor).unwrap();
    assert!(
        !text.contains("guardrail cursor") && text.contains("echo keep"),
        "cursor hooks after removal = {text}"
    );
    for tool in AiTool::ALL {
        assert!(hooks_ready(&home, tool).is_err(), "{tool} guardrail hook stayed");
    }
}
#[test]
fn install_tool_writes_the_command_for_that_tool_in_its_own_file() {
    let home_dir = tmp();
    let home = p(&home_dir);
    for tool in AiTool::ALL {
        install_tool(&home, "/k", tool).unwrap();
        let file = join(&home, dialect(tool).settings_file());
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.ends_with("}\n"), "{tool} file = {text}");
        assert!(
            text.contains(&format!("\\\"/k\\\" guardrail {}", tool.id())),
            "{tool} file = {text}"
        );
        hooks_ready(&home, tool).unwrap();
    }
    assert!(Path::new(&join(&home, ".cursor/hooks.json")).exists());
    assert!(Path::new(&join(&home, ".codex/hooks.json")).exists());
}

#[test]
fn hooks_ready_names_the_tool_and_the_missing_event() {
    let home_dir = tmp();
    let home = p(&home_dir);
    let err = hooks_ready(&home, AiTool::Claude).unwrap_err();
    assert_eq!(err.to_string(), "Claude guardrail hooks are not installed");
    install_tool(&home, "/k", AiTool::Claude).unwrap();
    let file = join(&home, ".claude/settings.json");
    let text = fs::read_to_string(&file).unwrap().replace("PostToolUse", "Renamed");
    fs::write(&file, text).unwrap();
    let err = hooks_ready(&home, AiTool::Claude).unwrap_err();
    assert_eq!(err.to_string(), "Claude guardrail hook PostToolUse is missing");
}

#[test]
fn a_hook_for_another_tool_does_not_make_this_one_ready() {
    let home_dir = tmp();
    let home = p(&home_dir);
    install_tool(&home, "/k", AiTool::Codex).unwrap();
    fs::copy(join(&home, ".codex/hooks.json"), {
        fs::create_dir_all(join(&home, ".claude")).unwrap();
        join(&home, ".claude/settings.json")
    })
    .unwrap();
    assert!(hooks_ready(&home, AiTool::Claude).is_err());
}

#[test]
fn remove_leaves_a_file_without_our_hooks_untouched() {
    let home_dir = tmp();
    let home = p(&home_dir);
    let file = join(&home, ".gemini/settings.json");
    fs::create_dir_all(join(&home, ".gemini")).unwrap();
    let original = "{\"hooks\":{\"BeforeTool\":[{\"hooks\":[{\"command\":\"echo keep\"}]}]}}";
    fs::write(&file, original).unwrap();
    remove_tool(&home, AiTool::Gemini).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    remove_tool(&home, AiTool::Cursor).unwrap();
    assert!(!Path::new(&join(&home, ".cursor")).exists());
}

#[test]
fn remove_reports_a_hooks_value_that_is_not_an_object() {
    let home_dir = tmp();
    let home = p(&home_dir);
    fs::create_dir_all(join(&home, ".cursor")).unwrap();
    fs::write(join(&home, ".cursor/hooks.json"), "{\"hooks\":[]}").unwrap();
    let err = remove_tool(&home, AiTool::Cursor).unwrap_err();
    assert_eq!(err.to_string(), "hooks are not an object");
    let err = remove(&home).unwrap_err();
    assert_eq!(err.to_string(), "Cursor: hooks are not an object");
}

#[test]
fn install_names_the_event_whose_list_is_not_a_list() {
    let home_dir = tmp();
    let home = p(&home_dir);
    fs::create_dir_all(join(&home, ".cursor")).unwrap();
    fs::write(join(&home, ".cursor/hooks.json"), "{\"hooks\":{\"preToolUse\":{}}}").unwrap();
    let err = install_tool(&home, "/k", AiTool::Cursor).unwrap_err();
    assert_eq!(err.to_string(), "preToolUse: hooks are not a list");
}

#[test]
fn cursor_install_adds_a_version_only_when_missing() {
    let home_dir = tmp();
    let home = p(&home_dir);
    let file = join(&home, ".cursor/hooks.json");
    fs::create_dir_all(join(&home, ".cursor")).unwrap();
    fs::write(&file, "{\"version\":7}").unwrap();
    install_tool(&home, "/k", AiTool::Cursor).unwrap();
    let root: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(root["version"], 7);
    let fresh_dir = tmp();
    let fresh = p(&fresh_dir);
    install_tool(&fresh, "/k", AiTool::Cursor).unwrap();
    let root: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(join(&fresh, ".cursor/hooks.json")).unwrap()).unwrap();
    assert_eq!(root["version"], 1);
}
