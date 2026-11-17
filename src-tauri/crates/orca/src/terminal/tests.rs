//! Terminal parsing and the prompt submit loop, against a scripted `orca` binary.

use super::prompt::unsent_prompt;
use super::*;
use crate::Client;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;

fn draft_text(raw: &[u8]) -> String {
    let trimmed = String::from_utf8_lossy(raw);
    let trimmed = trimmed.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return String::new();
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => draft_text_value(&value),
        Err(_) => String::new(),
    }
}

#[test]
fn parse_terminal_read_ok() {
    let view = parse_terminal_read(
        br#"{
		"ok": true,
		"result": {"terminal": {"handle": "term_a", "status": "running", "tail": ["hello", "world"]}}
	}"#,
    )
    .unwrap();
    assert!(
        view.handle == "term_a" && view.status == "running" && view.log == "hello\nworld",
        "view = {view:?}"
    );
    let err = parse_terminal_read(br#"{"ok":false,"error":{"message":"missing"}}"#).unwrap_err();
    assert_eq!(err.to_string(), "missing");
}

#[test]
fn parse_terminal_idle_cases() {
    let idle = parse_terminal_idle(br#"{"ok":true}"#).unwrap();
    assert!(idle);
    let idle = parse_terminal_idle(br#"{"ok":false,"error":{"code":"timeout","message":"timeout"}}"#).unwrap();
    assert!(!idle);
    let err = parse_terminal_idle(br#"{"ok":false,"error":{"code":"gone","message":"gone"}}"#).unwrap_err();
    assert_eq!(err.to_string(), "gone");
}

#[test]
fn unsent_prompt_cases() {
    assert!(
        unsent_prompt(&TerminalView {
            draft: "[Pasted text #1 +647 lines]".into(),
            ..Default::default()
        }),
        "draft was treated as sent"
    );
    assert!(
        unsent_prompt(&TerminalView {
            log: "Cursor Agent\n[Pasted text #1 +647 lines]".into(),
            ..Default::default()
        }),
        "pasted screen was treated as sent"
    );
    assert!(
        !unsent_prompt(&TerminalView {
            log: "working".into(),
            ..Default::default()
        }),
        "plain screen was treated as unsent"
    );
}

#[test]
fn draft_text_cases() {
    let got = draft_text(br#""[Pasted text #1 +647 lines]""#);
    assert_eq!(got, "[Pasted text #1 +647 lines]");
    let got = draft_text(br#"{"text":"hello"}"#);
    assert_eq!(got, "hello");
}

#[test]
fn submit_pasted_prompt_presses_enter() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("orca");
    let log = dir.path().join("args");
    let body = r#"#!/bin/sh
printf '%s\n' "$*" >> "$ORCA_ARGS"
case "$1 $2" in
  "terminal read")
printf '%s\n' '{"ok":true,"result":{"terminal":{"handle":"term_a","status":"running","draft":"[Pasted text #1 +647 lines]","tail":["Cursor Agent"]}}}'
;;
  "terminal wait")
printf '%s\n' '{"ok":true}'
;;
  "terminal send")
printf '%s\n' '{"ok":true}'
;;
  *)
echo unexpected >&2
exit 1
;;
esac
"#;
    std::fs::write(&script, body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let client = Client {
        bin: script.to_string_lossy().into_owned(),
        env: vec![format!("ORCA_ARGS={}", log.display())],
        ..Default::default()
    };
    Terminals::new(Cli::from(&client))
        .submit_pasted_prompt(&Ctx::background(), "term_a")
        .unwrap();
    let raw = std::fs::read_to_string(&log).unwrap();
    assert!(
        raw.contains("terminal send --terminal term_a --enter --wait-submit 8 --json"),
        "args =\n{raw}"
    );
}

#[test]
fn terminal_handle_from_launch() {
    let raw = r#"{"ok":true,"result":{"outcome":{"handle":"term_a"}}}"#;
    assert_eq!(terminal_handle(raw), "term_a");
}
