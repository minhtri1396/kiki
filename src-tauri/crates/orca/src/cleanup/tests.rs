//! `remove_task` against a scripted `orca` binary and a fake runtime.

use super::TaskScope;
use crate::wire::orca_gone;
use crate::Client;
use kiki_util::Ctx;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn remove_task() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("args");
    let script = dir.path().join("orca");
    let body = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
if [ "$1" = "status" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"app":{{"running":true}}}}}}'
  exit 0
fi
if [ "$1" = "project" ] && [ "$2" = "setups" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"setups":[{{"id":"setup_doc","displayName":"kiki-4-document","projectId":"p1","path":"/tmp/doc"}},{{"id":"setup_main","displayName":"kiki-document","projectId":"p0","path":"/tmp/main"}}]}}}}'
  exit 0
fi
if [ "$1" = "worktree" ] && [ "$2" = "list" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"worktrees":[{{"path":"/tmp/doc-wt","branch":"refs/heads/feat/GE-12-ship_billing-docs","displayName":"12"}}]}}}}'
  exit 0
fi
if [ "$1" = "worktree" ]; then
  printf '%s\n' '{{"ok":false,"error":{{"code":"selector_not_found","message":"selector_not_found"}}}}'
  exit 0
fi
if [ "$1" = "terminal" ]; then
  printf '%s\n' '{{"ok":false,"error":{{"code":"terminal_handle_stale","message":"terminal_handle_stale"}}}}'
  exit 0
fi
printf '%s\n' '{{"ok":true}}'
"#,
        log = log.display()
    );
    write_script(&script, &body);
    let methods: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let m = methods.clone();
    let client = Client {
        bin: script.to_string_lossy().into_owned(),
        runtime: Some(Arc::new(move |_ctx: &Ctx, method: &str, params: Option<&Value>| {
            m.lock().unwrap().push(method.to_string());
            if method == "projectGroup.list" {
                return Ok(json!({"groups":[{"id":"g1","name":"4"},{"id":"g2","name":"other"}]}));
            }
            let raw = serde_json::to_string(&params).unwrap();
            assert!(raw.contains(r#""groupId":"g1""#), "delete params = {raw}");
            Ok(json!({}))
        })),
        ..Default::default()
    };
    client
        .remove_task(
            &Ctx::background(),
            &TaskScope {
                id: "12".into(),
                group_id: "4".into(),
                workspace: "kiki".into(),
                slug: "ship_billing".into(),
                ticket: "GE-12".into(),
                repos: vec!["document".into(), "document".into()],
                folders: BTreeMap::from([("document".to_string(), "/tmp/doc".to_string())]),
                terminals: vec!["term_a".into(), "term_a".into()],
                ..Default::default()
            },
        )
        .unwrap();
    let got = std::fs::read_to_string(&log).unwrap();
    for want in [
        "terminal close --terminal term_a --tab --json",
        "worktree list --repo path:/tmp/doc --json",
        "worktree rm --worktree path:/tmp/doc-wt --json",
        "project setup-delete --setup setup_doc --force --json",
    ] {
        assert!(got.contains(want), "log missing {want:?}\n{got}");
    }
    assert!(
        !(got.contains("setup_main")
            || got.matches("terminal close").count() != 1
            || got.matches("worktree rm").count() != 1),
        "log = {got}"
    );
    assert_eq!(
        methods.lock().unwrap().join(","),
        "projectGroup.list,projectGroup.delete",
        "methods = {:?}",
        methods.lock().unwrap()
    );
}

#[test]
fn remove_task_keeps_shared_group() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("args");
    let script = dir.path().join("orca");
    let body = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
if [ "$1" = "status" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"app":{{"running":true}}}}}}'
  exit 0
fi
if [ "$1" = "project" ] && [ "$2" = "setups" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"setups":[{{"id":"setup_doc","displayName":"kiki-4-document","projectId":"p1","path":"/tmp/doc"}}]}}}}'
  exit 0
fi
if [ "$1" = "worktree" ] && [ "$2" = "list" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"worktrees":[{{"path":"/tmp/doc-wt","displayName":"12"}}]}}}}'
  exit 0
fi
printf '%s\n' '{{"ok":true}}'
"#,
        log = log.display()
    );
    write_script(&script, &body);
    let methods: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let m = methods.clone();
    let client = Client {
        bin: script.to_string_lossy().into_owned(),
        runtime: Some(Arc::new(move |_ctx: &Ctx, method: &str, _params: Option<&Value>| {
            m.lock().unwrap().push(method.to_string());
            Ok(json!({}))
        })),
        ..Default::default()
    };
    client
        .remove_task(
            &Ctx::background(),
            &TaskScope {
                id: "12".into(),
                group_id: "4".into(),
                workspace: "kiki".into(),
                slug: "ship_billing".into(),
                ticket: "GE-12".into(),
                repos: vec!["document".into()],
                folders: BTreeMap::from([("document".to_string(), "/tmp/doc".to_string())]),
                keep_group: true,
                keep_repos: vec!["document".into()],
                ..Default::default()
            },
        )
        .unwrap();
    let got = std::fs::read_to_string(&log).unwrap();
    assert!(
        got.contains("worktree rm --worktree path:/tmp/doc-wt --json"),
        "log = {got}"
    );
    assert!(
        !got.contains("setup-delete") && methods.lock().unwrap().is_empty(),
        "shared group was removed\nlog={got}\nmethods={:?}",
        methods.lock().unwrap()
    );
}

#[test]
fn orca_gone_cases() {
    assert!(
        orca_gone(br#"{"ok":false,"error":{"code":"runtime_error","message":"Project host setup not found: x"}}"#),
        "setup not found was kept"
    );
    assert!(
        !orca_gone(br#"{"ok":false,"error":{"code":"runtime_error","message":"still open"}}"#),
        "a real failure was ignored"
    );
}
