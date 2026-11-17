use std::path::Path;
use std::sync::{Arc, Mutex};

use kiki_aitool::AiTool;

use super::*;
use crate::system::{RunOutput, System};
use crate::toolbox::{hook_ready, remove_hooks, sync_hooks, Toolbox};

fn write(home: &Path, rel: &str, body: &str) {
    let path = home.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[test]
fn hook_ready_accepts_the_rtk_command_hook() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    write(
        home,
        ".cursor/hooks.json",
        r#"{"hooks":{"preToolUse":[{"command":"/opt/homebrew/bin/rtk hook cursor","matcher":"Shell"}]}}"#,
    );
    write(
        home,
        ".claude/settings.json",
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}]}}"#,
    );
    write(
        home,
        ".codex/hooks.json",
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook codex"}]}]}}"#,
    );
    write(
        home,
        ".gemini/settings.json",
        &format!(
            r#"{{"hooks":{{"BeforeTool":[{{"matcher":"run_shell_command","hooks":[{{"type":"command","command":"{}"}}]}}]}}}}"#,
            home.join(".gemini").join("hooks").join("rtk-hook-gemini.sh").display()
        ),
    );
    write(home, ".gemini/hooks/rtk-hook-gemini.sh", "#!/bin/sh\n");
    for tool in AiTool::ALL {
        if let Err(e) = hook_ready(&home.to_string_lossy(), tool) {
            panic!("{tool}: {e:#}");
        }
    }
}

#[test]
fn hook_ready_reports_a_missing_rtk_hook() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let h = home.to_string_lossy().to_string();
    write(
        home,
        ".cursor/hooks.json",
        r#"{"hooks":{"preToolUse":[{"command":"rtk hook cursor","matcher":"Read"}]}}"#,
    );
    let err = hook_ready(&h, AiTool::Cursor).unwrap_err().to_string();
    assert!(err.contains("Cursor RTK hook preToolUse is missing"), "err = {err}");
    let err = hook_ready(&h, AiTool::Claude).unwrap_err().to_string();
    assert!(err.contains("Claude RTK hook is not installed"), "claude = {err}");
    write(
        home,
        ".gemini/settings.json",
        r#"{"hooks":{"BeforeTool":[{"matcher":"run_shell_command","hooks":[{"command":"rtk-hook-gemini.sh"}]}]}}"#,
    );
    let err = hook_ready(&h, AiTool::Gemini).unwrap_err().to_string();
    assert!(err.contains("Gemini RTK hook script is missing"), "gemini = {err}");
}

#[test]
fn remove_hooks_drops_rtk_and_keeps_other_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let h = home.to_string_lossy().to_string();
    write(
        home,
        ".cursor/hooks.json",
        r#"{"version":1,"hooks":{"preToolUse":[{"command":"rtk hook cursor","matcher":"Shell"},{"command":"/bin/kiki guardrail cursor"}]}}"#,
    );
    write(
        home,
        ".claude/settings.json",
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"command":"/opt/homebrew/bin/rtk hook claude"}]},{"hooks":[{"command":"/bin/kiki guardrail claude"}]}]}}"#,
    );
    write(
        home,
        ".codex/hooks.json",
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"command":"rtk hook codex"},{"command":"/bin/kiki guardrail codex"}]}]}}"#,
    );
    write(
        home,
        ".gemini/settings.json",
        &format!(
            r#"{{"hooks":{{"BeforeTool":[{{"matcher":"run_shell_command","hooks":[{{"command":"{}"}}]}},{{"hooks":[{{"command":"/bin/kiki guardrail gemini"}}]}}]}}}}"#,
            home.join(".gemini").join("hooks").join("rtk-hook-gemini.sh").display()
        ),
    );
    write(home, ".gemini/hooks/rtk-hook-gemini.sh", "#!/bin/sh\n");
    remove_hooks(&h).unwrap();
    for (rel, hook, keep) in [
        (".cursor/hooks.json", "rtk hook cursor", "/bin/kiki guardrail cursor"),
        (".claude/settings.json", "rtk hook claude", "/bin/kiki guardrail claude"),
        (".codex/hooks.json", "rtk hook codex", "/bin/kiki guardrail codex"),
        (
            ".gemini/settings.json",
            "rtk-hook-gemini.sh",
            "/bin/kiki guardrail gemini",
        ),
    ] {
        let body = std::fs::read_to_string(home.join(rel)).unwrap();
        assert!(!body.contains(hook) && body.contains(keep), "{rel} =\n{body}");
    }
    assert!(
        std::fs::metadata(home.join(".gemini/hooks/rtk-hook-gemini.sh")).is_err(),
        "gemini script stayed"
    );
    for tool in AiTool::ALL {
        assert!(hook_ready(&h, tool).is_err(), "{tool} hook stayed ready");
    }
}

/// A machine with a fixed set of commands whose `rtk` runs are recorded (or forbidden).
struct FakeSystem {
    commands: Vec<&'static str>,
    calls: Mutex<Vec<String>>,
    allow_runs: bool,
}

impl FakeSystem {
    fn new(commands: &[&'static str], allow_runs: bool) -> Arc<FakeSystem> {
        Arc::new(FakeSystem {
            commands: commands.to_vec(),
            calls: Mutex::new(Vec::new()),
            allow_runs,
        })
    }

    fn toolbox(self: &Arc<Self>) -> Toolbox {
        Toolbox::new(self.clone())
    }
}

impl System for FakeSystem {
    fn command_exists(&self, name: &str) -> bool {
        self.commands.contains(&name)
    }

    fn run_rtk(&self, home: &str, args: &[String]) -> RunOutput {
        assert!(self.allow_runs, "rtk ran while it was missing and off");
        assert!(!home.is_empty(), "home is empty");
        self.calls.lock().unwrap().push(args.join(" "));
        RunOutput {
            output: Vec::new(),
            result: Ok(()),
        }
    }
}

#[test]
fn ensure_hook_runs_the_tool_install() {
    let system = FakeSystem::new(&["rtk"], true);
    let toolbox = system.toolbox();
    let home = tempfile::tempdir().unwrap();
    toolbox
        .ensure_hook(&home.path().to_string_lossy(), AiTool::Cursor)
        .unwrap();
    let want = "init --global --agent cursor --hook-only --auto-patch";
    assert_eq!(
        *system.calls.lock().unwrap(),
        vec![want.to_string()],
        "calls = {:?}",
        system.calls.lock().unwrap()
    );
    let none = FakeSystem::new(&[], false);
    let home2 = tempfile::tempdir().unwrap();
    let err = none
        .toolbox()
        .ensure_hook(&home2.path().to_string_lossy(), AiTool::Claude)
        .unwrap_err();
    assert_eq!(err.to_string(), "rtk is not installed");
}

#[test]
fn sync_hooks_skips_when_rtk_is_missing_and_off() {
    let system = FakeSystem::new(&[], false);
    let toolbox = system.toolbox();
    let home = tempfile::tempdir().unwrap();
    toolbox.sync_hooks(&home.path().to_string_lossy(), false).unwrap();
    let home2 = tempfile::tempdir().unwrap();
    let err = toolbox
        .sync_hooks(&home2.path().to_string_lossy(), true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("not installed"), "err = {err}");
}

#[test]
fn sync_hooks_runs_install_then_remove() {
    let system = FakeSystem::new(&["rtk"], true);
    let toolbox = system.toolbox();
    let home = tempfile::tempdir().unwrap();
    let h = home.path().to_string_lossy().to_string();
    toolbox.sync_hooks(&h, true).unwrap();
    toolbox.sync_hooks(&h, false).unwrap();
    let mut want = install_hook_steps();
    want.extend(remove_hook_steps());
    let got = system.calls.lock().unwrap();
    assert_eq!(got.len(), want.len(), "calls = {got:?}");
    for (i, args) in want.iter().enumerate() {
        assert_eq!(got[i], args.join(" "), "call {i}");
    }
}

#[test]
fn sync_hooks_installs_beside_an_existing_guardrail() {
    if kiki_util::look_path("rtk").is_none() {
        eprintln!("rtk is not installed; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let h = home.to_string_lossy().to_string();
    for d in [".cursor", ".claude", ".codex", ".gemini"] {
        std::fs::create_dir_all(home.join(d)).unwrap();
    }
    write(
        home,
        ".cursor/hooks.json",
        &(r#"{"version":1,"hooks":{"preToolUse":[{"command":"/bin/kiki guardrail cursor"}]}}"#.to_string() + "\n"),
    );
    write(
        home,
        ".claude/settings.json",
        &(r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"/bin/kiki guardrail claude"}]}]}}"#
            .to_string()
            + "\n"),
    );
    write(
        home,
        ".codex/hooks.json",
        &(r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"/bin/kiki guardrail codex"}]}]}}"#
            .to_string()
            + "\n"),
    );
    write(home, ".gemini/settings.json", &(r#"{"hooks":{"BeforeTool":[{"hooks":[{"type":"command","command":"/bin/kiki guardrail gemini","name":"kiki-guardrail"}]}]}}"#.to_string() + "\n"));

    sync_hooks(&h, true).unwrap();
    let items = [
        (".cursor/hooks.json", "rtk hook cursor", "/bin/kiki guardrail cursor"),
        (".claude/settings.json", "rtk hook claude", "/bin/kiki guardrail claude"),
        (".codex/hooks.json", "rtk hook codex", "/bin/kiki guardrail codex"),
        (
            ".gemini/settings.json",
            "rtk-hook-gemini.sh",
            "/bin/kiki guardrail gemini",
        ),
    ];
    for (rel, hook, guard) in items {
        let body = std::fs::read_to_string(home.join(rel)).unwrap();
        assert!(body.contains(hook) && body.contains(guard), "{rel} =\n{body}");
    }
    sync_hooks(&h, false).unwrap();
    for (rel, hook, guard) in items {
        let body = std::fs::read_to_string(home.join(rel)).unwrap();
        assert!(
            !body.contains(hook) && body.contains(guard),
            "after removal {rel} =\n{body}"
        );
    }
}

#[test]
fn installed_uses_the_command_check() {
    let system = FakeSystem::new(&["gh"], false);
    let toolbox = system.toolbox();
    assert!(toolbox.installed("gh") && !toolbox.installed("rtk") && !toolbox.installed("  "));
}

#[test]
fn every_tool_has_a_dialect_and_install_steps() {
    for tool in AiTool::ALL {
        let spec = for_tool(tool).spec();
        assert!(!spec.title.is_empty() && !spec.config_rel.is_empty(), "{tool}");
        assert_eq!(install_steps(tool).first().map(String::as_str), Some("init"), "{tool}");
    }
    assert_eq!(install_hook_steps().len(), 4);
    assert_eq!(remove_hook_steps().len(), 3);
}

#[test]
fn remove_reports_unreadable_hooks_files() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let h = home.to_string_lossy().to_string();
    write(
        home,
        ".cursor/hooks.json",
        r#"{"hooks":{"preToolUse":{"command":"x"}}}"#,
    );
    write(home, ".claude/settings.json", r#"{"hooks":[1]}"#);
    write(home, ".codex/hooks.json", "[1]");
    let err = remove_hooks(&h).unwrap_err().to_string();
    assert_eq!(
        err,
        "RTK hook: Cursor preToolUse: hooks are not a list; Claude RTK hook: hooks are not an object; \
         Codex RTK hook: json: cannot unmarshal array into Go value of type map[string]interface {}"
    );
}

#[test]
fn removing_rtk_rewrites_the_file_html_safe_with_a_trailing_newline() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let h = home.to_string_lossy().to_string();
    write(
        home,
        ".cursor/hooks.json",
        r#"{"hooks":{"preToolUse":[{"command":"rtk hook cursor","matcher":"Shell"},{"command":"a<b"}]}}"#,
    );
    remove_hooks(&h).unwrap();
    let body = std::fs::read_to_string(home.join(".cursor/hooks.json")).unwrap();
    assert_eq!(body, "{\n  \"hooks\": {\n    \"preToolUse\": [\n      {\n        \"command\": \"a\\u003cb\"\n      }\n    ]\n  }\n}\n");
}
