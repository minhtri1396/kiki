//! Pattern: Strategy + Factory Method. A [`HookDialect`] says where one AI tool keeps its hooks and how an RTK entry
//! looks inside them; [`for_tool`] picks the dialect. Cursor lists hooks flat, the other tools group them under
//! a matcher, so those two shapes differ in how an entry is found and stripped.

use kiki_aitool::AiTool;
use serde_json::Value;

use super::edit;

/// The facts about one tool's RTK hook.
pub(crate) struct HookSpec {
    /// Display name used in messages.
    pub title: &'static str,
    /// The tool's hooks file, relative to the home folder.
    pub config_rel: &'static str,
    /// The hook event RTK registers under.
    pub event: &'static str,
    /// Text a hook command contains when it is RTK's.
    pub command: &'static str,
    /// The tool name RTK's hook applies to (compared ignoring case).
    pub matcher: &'static str,
    /// `rtk` arguments that register the hook.
    pub install_args: &'static [&'static str],
    /// `rtk` arguments that remove it, for tools where `rtk` can.
    pub uninstall_args: Option<&'static [&'static str]>,
    /// A script RTK writes that the hook calls, relative to home.
    pub script: Option<&'static str>,
    /// Files removed together with the script.
    pub script_leftovers: &'static [&'static str],
}

/// How one AI tool lays out its hooks file.
pub(crate) trait HookDialect: Sync {
    fn spec(&self) -> &'static HookSpec;

    /// Whether the event's hook list holds RTK's hook.
    fn event_has_rtk(&self, event: Option<&Value>) -> bool;

    /// A list entry without RTK's hook: `None` when the whole entry goes away, and whether anything was removed.
    fn strip_item(&self, item: &Value) -> (Option<Value>, bool);

    /// Reports that the tool has RTK's hook, as `Ok(())`, or why not.
    fn ready(&self, home: &str) -> anyhow::Result<()> {
        edit::ready(self, home)
    }

    /// Removes RTK's hook and script; other hooks stay in place.
    fn uninstall(&self, home: &str) -> anyhow::Result<()> {
        edit::uninstall(self, home)
    }
}

/// Picks the dialect for an AI tool.
pub(crate) fn for_tool(tool: AiTool) -> &'static dyn HookDialect {
    match tool {
        AiTool::Cursor => &CURSOR,
        AiTool::Claude => &CLAUDE,
        AiTool::Codex => &CODEX,
        AiTool::Gemini => &GEMINI,
    }
}

/// Cursor: `preToolUse: [{"command": ..., "matcher": ...}]`.
struct Flat(&'static HookSpec);

/// Claude, Codex, Gemini: `PreToolUse: [{"matcher": ..., "hooks": [{"command": ...}]}]`.
struct Grouped(&'static HookSpec);

static CURSOR_SPEC: HookSpec = HookSpec {
    title: "Cursor",
    config_rel: ".cursor/hooks.json",
    event: "preToolUse",
    command: "rtk hook cursor",
    matcher: "Shell",
    install_args: &["init", "--global", "--agent", "cursor", "--hook-only", "--auto-patch"],
    uninstall_args: None,
    script: None,
    script_leftovers: &[],
};

static CURSOR: Flat = Flat(&CURSOR_SPEC);

static CLAUDE_SPEC: HookSpec = HookSpec {
    title: "Claude",
    config_rel: ".claude/settings.json",
    event: "PreToolUse",
    command: "rtk hook claude",
    matcher: "Bash",
    install_args: &["init", "--global", "--hook-only", "--auto-patch"],
    uninstall_args: Some(&["init", "--global", "--uninstall", "--auto-patch"]),
    script: None,
    script_leftovers: &[],
};

static CLAUDE: Grouped = Grouped(&CLAUDE_SPEC);

static CODEX_SPEC: HookSpec = HookSpec {
    title: "Codex",
    config_rel: ".codex/hooks.json",
    event: "PreToolUse",
    command: "rtk hook codex",
    matcher: "Bash",
    install_args: &["init", "--global", "--codex"],
    uninstall_args: Some(&["init", "--global", "--codex", "--uninstall"]),
    script: None,
    script_leftovers: &[],
};

static CODEX: Grouped = Grouped(&CODEX_SPEC);

static GEMINI_SPEC: HookSpec = HookSpec {
    title: "Gemini",
    config_rel: ".gemini/settings.json",
    event: "BeforeTool",
    command: "rtk-hook-gemini.sh",
    matcher: "run_shell_command",
    install_args: &["init", "--global", "--gemini", "--hook-only", "--auto-patch"],
    uninstall_args: Some(&["init", "--global", "--gemini", "--uninstall", "--auto-patch"]),
    script: Some(".gemini/hooks/rtk-hook-gemini.sh"),
    script_leftovers: &[".gemini/hooks/.rtk-hook.sha256"],
};

static GEMINI: Grouped = Grouped(&GEMINI_SPEC);

impl HookDialect for Flat {
    fn spec(&self) -> &'static HookSpec {
        self.0
    }

    fn event_has_rtk(&self, event: Option<&Value>) -> bool {
        let Some(Value::Array(list)) = event else { return false };
        list.iter().any(|item| {
            command_matches(item.get("command"), self.0.command) && matcher_matches(item.get("matcher"), self.0.matcher)
        })
    }

    fn strip_item(&self, item: &Value) -> (Option<Value>, bool) {
        match item {
            Value::Object(obj) if command_matches(obj.get("command"), self.0.command) => (None, true),
            _ => (Some(item.clone()), false),
        }
    }
}

impl HookDialect for Grouped {
    fn spec(&self) -> &'static HookSpec {
        self.0
    }

    fn event_has_rtk(&self, event: Option<&Value>) -> bool {
        let Some(Value::Array(list)) = event else { return false };
        list.iter().any(|item| {
            matcher_matches(item.get("matcher"), self.0.matcher)
                && matches!(item.get("hooks"), Some(Value::Array(inner))
                    if inner.iter().any(|hook| command_matches(hook.get("command"), self.0.command)))
        })
    }

    fn strip_item(&self, item: &Value) -> (Option<Value>, bool) {
        let Value::Object(obj) = item else {
            return (Some(item.clone()), false);
        };
        let Some(Value::Array(inner)) = obj.get("hooks") else {
            return (Some(item.clone()), false);
        };
        let mut removed = false;
        let mut kept: Vec<Value> = Vec::new();
        for hook in inner {
            if matches!(hook, Value::Object(row) if command_matches(row.get("command"), self.0.command)) {
                removed = true;
                continue;
            }
            kept.push(hook.clone());
        }
        if kept.is_empty() {
            return (None, removed);
        }
        let mut obj = obj.clone();
        obj.insert("hooks".to_string(), Value::Array(kept));
        (Some(Value::Object(obj)), removed)
    }
}

fn command_matches(value: Option<&Value>, want: &str) -> bool {
    value.and_then(|v| v.as_str()).unwrap_or("").contains(want)
}

fn matcher_matches(value: Option<&Value>, want: &str) -> bool {
    let got = value.and_then(|v| v.as_str()).unwrap_or("");
    got.trim().to_lowercase() == want.to_lowercase()
}
