use kiki_aitool::AiTool;

use super::inspect::inspect;
use super::*;
use crate::endpoint::{publish, Endpoint};
use crate::testutil::{p, tmp};

#[test]
fn gate_allows_when_the_app_is_down_and_blocks_when_it_refuses() {
    let home = tmp();
    let repo = tmp();
    let other = tmp();
    let (home, repo, other) = (p(&home), p(&repo), p(&other));
    publish(
        &home,
        Endpoint {
            url: "http://127.0.0.1:9".into(),
            workspace: "kiki".into(),
            paths: vec![repo.clone()],
        },
    )
    .unwrap();
    let down = |_: &str, _: &[u8]| -> anyhow::Result<(bool, String)> { Err(anyhow::anyhow!("file does not exist")) };
    let owned = gate(
        &home,
        AiTool::Cursor,
        format!(r#"{{"cwd":"{repo}","hook_event_name":"beforeShellExecution","command":"ls"}}"#).as_bytes(),
        "",
        down,
    );
    assert!(owned.contains(r#""permission":"allow""#), "app down = {owned}");
    let called = std::cell::Cell::new(false);
    let outside = gate(
        &home,
        AiTool::Cursor,
        format!(r#"{{"cwd":"{other}","command":"ls"}}"#).as_bytes(),
        "",
        |_: &str, _: &[u8]| -> anyhow::Result<(bool, String)> {
            called.set(true);
            Err(anyhow::anyhow!("file does not exist"))
        },
    );
    assert!(
        !called.get() && outside.contains(r#""permission":"allow""#),
        "outside called={} reply={outside}",
        called.get()
    );
    let refused = gate(
        &home,
        AiTool::Cursor,
        format!(r#"{{"cwd":"{repo}","hook_event_name":"beforeMCPExecution","server":"mcp-other"}}"#).as_bytes(),
        "",
        |_: &str, _: &[u8]| -> anyhow::Result<(bool, String)> {
            Ok((
                false,
                "Guardrail: this agent cannot use the MCP server mcp-other.".to_string(),
            ))
        },
    );
    assert!(
        refused.contains(r#""permission":"deny""#) && refused.contains("mcp-other"),
        "refused = {refused}"
    );
}
#[test]
fn inspect_reads_event_cwd_and_absolute_paths() {
    let got =
        inspect(br#"{"hook_event_name":"beforeReadFile","cwd":"/work","file_path":"/work/a.rs","url":"http://x/y"}"#);
    assert_eq!(got.event, "beforeReadFile");
    assert_eq!(got.cwd, "/work");
    assert!(got.paths.contains(&"/work".to_string()) && got.paths.contains(&"/work/a.rs".to_string()));
    assert!(!got.paths.iter().any(|path| path.contains("://")));
}

#[test]
fn inspect_falls_back_to_the_first_absolute_workspace_root() {
    let got = inspect(br#"{"cwd":"relative","workspace_roots":["rel"," /one ","/two"]}"#);
    assert_eq!(got.cwd, "/one");
    assert!(inspect(b"not json").cwd.is_empty());
}

#[test]
fn reduce_keeps_decision_fields_and_truncates_long_text() {
    let long = "é".repeat(5000);
    let raw = format!(
        r#"{{"tool_name":"Edit","prompt":"secret","tool_input":{{"path":"/a","content":"body","command":"{long}"}},"workspace_roots":["/a"]}}"#
    );
    let kept: Value = serde_json::from_slice(&reduce(raw.as_bytes())).unwrap();
    assert_eq!(kept["tool_name"], "Edit");
    assert!(kept.get("prompt").is_none());
    assert_eq!(kept["tool_input"]["path"], "/a");
    assert!(kept["tool_input"].get("content").is_none());
    assert_eq!(kept["tool_input"]["command"].as_str().unwrap().len(), 8000);
    assert_eq!(kept["workspace_roots"], json!(["/a"]));
}

#[test]
fn gate_posts_the_reduced_payload_with_the_tool_id() {
    let home_dir = tmp();
    let home = p(&home_dir);
    let repo_dir = tmp();
    let repo = p(&repo_dir);
    publish(
        &home,
        Endpoint {
            url: "http://app".into(),
            workspace: "kiki".into(),
            paths: vec![repo.clone()],
        },
    )
    .unwrap();
    let seen = std::cell::RefCell::new(String::new());
    let out = gate(
        &home,
        AiTool::Gemini,
        format!(r#"{{"cwd":"{repo}","hook_event_name":"BeforeTool","prompt":"x"}}"#).as_bytes(),
        "reader",
        |url: &str, body: &[u8]| {
            *seen.borrow_mut() = format!("{url} {}", String::from_utf8_lossy(body));
            Ok((false, "no".to_string()))
        },
    );
    assert_eq!(out, "{\"decision\":\"deny\",\"reason\":\"no\"}\n");
    let seen = seen.borrow();
    assert!(seen.starts_with("http://app "), "seen = {seen}");
    let body: Value = serde_json::from_str(&seen["http://app ".len()..]).unwrap();
    assert_eq!(body["tool"], "gemini");
    assert_eq!(body["agent"], "reader");
    assert_eq!(body["cwd"], repo.as_str());
    assert_eq!(body["event"], "BeforeTool");
    assert!(body["input"].get("prompt").is_none());
}
