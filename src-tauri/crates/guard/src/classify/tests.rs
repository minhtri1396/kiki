use super::*;
use crate::policy::TargetKind;

fn cl(event: &str, raw: &str) -> Vec<Target> {
    classify(event, raw.as_bytes())
}

fn kinds(targets: &[Target]) -> Vec<(TargetKind, &str)> {
    targets.iter().map(|t| (t.kind, t.name.as_str())).collect()
}

#[test]
fn events_choose_the_kind_of_target() {
    let mcp = cl(
        "beforeMCPExecution",
        r#"{"server":"mcp-atlassian-kiki","tool_name":"jira_search"}"#,
    );
    assert_eq!(kinds(&mcp), [(TargetKind::Mcp, "mcp-atlassian-kiki jira_search")]);
    let read = cl("beforeReadFile", r#"{"file_path":"/a/b"}"#);
    assert_eq!(read, [Target::repo(vec!["/a/b".into()], false)]);
    let edit = cl("afterFileEdit", r#"{"file_path":"/a/b"}"#);
    assert_eq!(edit, [Target::repo(vec!["/a/b".into()], true)]);
    let agent = cl("subagentStart", r#"{"subagent_type":"reviewer"}"#);
    assert_eq!(kinds(&agent), [(TargetKind::Agent, "reviewer")]);
    assert!(cl("UserPromptSubmit", r#"{"prompt":"x"}"#).is_empty());
}

#[test]
fn the_event_comes_from_the_payload_when_not_given() {
    let got = cl("", r#"{"hook_event_name":"BeforeReadFile","path":"/x"}"#);
    assert_eq!(got, [Target::repo(vec!["/x".into()], false)]);
}

#[test]
fn tool_calls_are_sorted_by_tool_name() {
    let mcp = cl("PreToolUse", r#"{"tool_name":"mcp__jira__search"}"#);
    assert_eq!(kinds(&mcp), [(TargetKind::Mcp, "mcp__jira__search")]);
    let task = cl(
        "PreToolUse",
        r#"{"tool_name":"Task","tool_input":{"subagent_type":"planner"}}"#,
    );
    assert_eq!(kinds(&task), [(TargetKind::Agent, "planner")]);
    let other = cl("PreToolUse", r#"{"tool_name":"WebSearch"}"#);
    assert_eq!(kinds(&other), [(TargetKind::Tool, "WebSearch")]);
    let write = cl("PreToolUse", r#"{"tool_name":"Write","tool_input":{"path":"/a"}}"#);
    assert_eq!(write, [Target::repo(vec!["/a".into()], true)]);
    let read = cl("PreToolUse", r#"{"tool_name":"Read","tool_input":{"path":"/a"}}"#);
    assert_eq!(read, [Target::repo(vec!["/a".into()], false)]);
}

#[test]
fn unknown_events_go_down_the_inference_chain() {
    let command = cl("custom", r#"{"command":"ls /a"}"#);
    assert_eq!(command, [Target::repo(vec!["/a".into()], false)]);
    let server = cl("custom", r#"{"server":"s"}"#);
    assert_eq!(kinds(&server), [(TargetKind::Mcp, "s")]);
    let tool = cl("custom", r#"{"tool_name":"Tool"}"#);
    assert_eq!(kinds(&tool), [(TargetKind::Tool, "Tool")]);
    let paths = cl("custom", r#"{"files":["/a","/b"]}"#);
    assert_eq!(paths, [Target::repo(vec!["/a".into(), "/b".into()], false)]);
    assert!(cl("custom", r#"{"nothing":1}"#).is_empty());
    assert!(cl("", "not json").is_empty());
}

#[test]
fn a_command_with_a_tool_name_is_not_a_bare_command() {
    let got = cl("custom", r#"{"command":"ls","tool_name":"Thing"}"#);
    assert_eq!(kinds(&got), [(TargetKind::Tool, "Thing")]);
}

#[test]
fn shell_commands_add_cli_mcp_and_agent_targets() {
    let got = cl(
        "beforeShellExecution",
        r#"{"command":"sudo gh pr list && rtk gh pr view; ls ~/x >out.txt"}"#,
    );
    assert_eq!(got[0].kind, TargetKind::Repo);
    assert!(got[0].write, "redirect writes");
    assert_eq!(got[0].paths, ["~/x", "out.txt"]);
    let names: Vec<_> = kinds(&got[1..]);
    assert_eq!(names, [(TargetKind::Tool, "GitHub CLI"), (TargetKind::Tool, "RTK")]);
    let mcp = cl(
        "beforeShellExecution",
        r#"{"command":"cat mcp-codegraph-kiki MCP-codegraph-kiki"}"#,
    );
    assert_eq!(kinds(&mcp[1..]), [(TargetKind::Mcp, "mcp-codegraph-kiki")]);
    let launch = cl(
        "beforeShellExecution",
        r#"{"command":"orca agent launch --agent reviewer"}"#,
    );
    assert_eq!(kinds(&launch[1..]), [(TargetKind::Agent, "reviewer")]);
}

#[test]
fn writing_commands() {
    use super::writes::command_writes;
    for command in [
        "touch a",
        "git commit -m x",
        "git branch -D x",
        "git remote add a b",
        "git config user.name x",
        "git stash",
        "git tag v1",
        "sed -i s/a/b/ f",
        "find . -delete",
        "echo hi > f",
        "ls; rm x",
        "codegraph sync .",
    ] {
        assert!(command_writes(command), "{command} should write");
    }
    for command in [
        "",
        "ls -la",
        "git status",
        "git log --oneline",
        "git branch",
        "git remote -v",
        "git config --get a",
        "git stash list",
        "git tag -l",
        "sed s/a/b/ f",
        "find . -name x",
        "echo hi 2>&1",
        "gh pr list",
        "rtk git status",
        "codegraph explore q",
        "FOO=1 sudo cat x | wc -l",
    ] {
        assert!(!command_writes(command), "{command} should not write");
    }
}

#[test]
fn command_lexing() {
    use super::tokens::{segment_command, split_command, tokenize};
    assert_eq!(
        split_command("a; b && c || d | e\nf 'x;y'"),
        ["a", "b", "c", "d", "e", "f 'x;y'"]
    );
    assert_eq!(tokenize("cat \"a b\" 'c' d"), ["cat", "a b", "c", "d"]);
    let tokens = tokenize("FOO=1 sudo -n /usr/bin/Git status");
    let (base, rest) = segment_command(&tokens);
    assert_eq!(base, "git");
    assert_eq!(rest, ["status"]);
    assert_eq!(segment_command(&[]).0, "");
}
