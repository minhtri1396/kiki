use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use kiki_orca as orca;
use kiki_store as store;
use serde::Deserialize;
use serde_json::{json, Value};

use super::branch::{branch_slug, jira_ticket};
use super::test_support::{call, create_task, new_task_test_server, state, task_test_server, write_script, TaskBody};

#[test]
fn task_api() {
    let (app, _tmp) = new_task_test_server();
    let snapshot = state(&app);
    assert_eq!(
        snapshot["tasks"].as_array().map(Vec::len),
        Some(0),
        "tasks = {:?}",
        snapshot["tasks"]
    );

    let created = create_task(
        &app,
        r#"{"name":"Ship billing","description":"Add invoices.","agents":[{"agentId":"requirement_planner","status":"pending"},{"agentId":"backend_runner","status":"doing"}]}"#,
    );
    assert!(
        created.column == store::Column::Todo && created.agents.len() == 2,
        "created = {created:?}"
    );

    let id = created.id.clone();
    let (status, text) = call(
        &app,
        "PUT",
        &format!("/api/tasks/{id}"),
        Some(
            r#"{"name":"Ship billing","description":"Add invoices.","column":"done","agents":[{"agentId":"backend_runner","status":"completed"}]}"#,
        ),
    );
    assert_eq!(status, 200, "update status = {status} body = {text}");
    let updated = serde_json::from_str::<TaskBody>(&text).unwrap().task;
    assert!(
        updated.column == store::Column::Todo
            && updated.done_at.is_empty()
            && !updated.started
            && updated.agents.len() == 1
            && updated.agents[0].status == store::AgentStatus::Pending,
        "updated = {updated:?}"
    );

    let (status, text) = call(&app, "POST", "/api/tasks", Some(r#"{"name":"","description":"x"}"#));
    assert_eq!(status, 400, "empty name status = {status} body = {text}");

    let (status, text) = call(
        &app,
        "POST",
        "/api/tasks",
        Some(r#"{"name":"Missing","agents":[{"agentId":"not_an_agent","status":"pending"}]}"#),
    );
    assert!(
        status == 400 && text.contains("not an agent"),
        "unknown agent status = {status} body = {text}"
    );

    let snapshot = state(&app);
    let tasks = snapshot["tasks"].as_array().unwrap();
    assert!(
        tasks.len() == 1 && tasks[0]["name"] == "Ship billing",
        "snapshot tasks = {tasks:?}"
    );

    let (status, text) = call(&app, "DELETE", &format!("/api/tasks/{id}"), None);
    assert_eq!(status, 200, "delete status = {status} body = {text}");
    let left = app.store.tasks().unwrap();
    assert!(left.is_empty(), "left = {left:?}");
}

#[test]
fn task_branch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let (app, _tmp) = task_test_server(|builder| {
        builder.branch_namer(move |_ctx, _tool, _model, name| {
            counter.fetch_add(1, Ordering::SeqCst);
            if name == "Ship the cart" {
                return Ok("Ship the cart".to_string());
            }
            Ok("Add shopping cart.".to_string())
        })
    });
    let created = create_task(&app, r#"{"name":"GE-12 Thêm giỏ hàng","description":"See also ZZ-9."}"#);
    let id = created.id.clone();
    let branch = || -> Value {
        let (status, text) = call(&app, "POST", &format!("/api/tasks/{id}/branch"), None);
        assert_eq!(status, 200, "branch status = {status} body = {text}");
        serde_json::from_str(&text).unwrap()
    };
    let first = branch();
    assert!(
        first["slug"] == "add_shopping_cart" && first["ticket"] == "GE-12" && calls.load(Ordering::SeqCst) == 1,
        "first = {first:?} calls = {}",
        calls.load(Ordering::SeqCst)
    );
    let second = branch();
    assert!(
        second["slug"] == "add_shopping_cart" && calls.load(Ordering::SeqCst) == 1,
        "second = {second:?} calls = {}",
        calls.load(Ordering::SeqCst)
    );
    app.store
        .update_task(
            &created.id,
            store::TaskInput {
                name: "Ship the cart".into(),
                description: "Follow ZZ-9.".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let third = branch();
    assert!(
        third["slug"] == "ship_the_cart" && third["ticket"] == "ZZ-9" && calls.load(Ordering::SeqCst) == 2,
        "third = {third:?} calls = {}",
        calls.load(Ordering::SeqCst)
    );
}

#[test]
fn delete_task_removes_orca() {
    let (mut app, tmp) = new_task_test_server();
    let group = app
        .store
        .create_group(store::GroupInput {
            name: "Billing".into(),
            epic_key: "KIKI-8".into(),
            epic_board_id: "12".into(),
            epic_board_name: "KIKI".into(),
            ..Default::default()
        })
        .unwrap();
    let created = create_task(
        &app,
        &json!({
            "name": "GE-12 Ship billing",
            "description": "Add invoices.",
            "groupId": group.id,
            "agents": [{"agentId": "requirement_planner"}],
        })
        .to_string(),
    );
    app.store
        .set_task_branch(&created.id, "ship_billing", "GE-12 Ship billing")
        .unwrap();
    app.store
        .save_task_run(&created.id, "requirement_planner", "term_plan")
        .unwrap();
    let id = created.id.clone();
    let log = tmp.path().join("args");
    let script = tmp.path().join("orca");
    let body = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
if [ "$1" = "status" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"app":{{"running":true}}}}}}'
  exit 0
fi
if [ "$1" = "project" ] && [ "$2" = "setups" ]; then
  printf '%s\n' '{{"ok":true,"result":{{"setups":[{{"id":"setup_doc","displayName":"kiki-{gid}-document","projectId":"p1","path":"/tmp/doc"}}]}}}}'
  exit 0
fi
printf '%s\n' '{{"ok":true}}'
"#,
        log = log.display(),
        gid = group.id
    );
    write_script(&script, &body);
    let methods: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = methods.clone();
    let group_name = group.id.clone();
    app.orca = orca::Client {
        bin: script.to_string_lossy().into_owned(),
        runtime: Some(Arc::new(move |_ctx, method, _params| {
            seen.lock().unwrap().push(method.to_string());
            if method == "projectGroup.list" {
                return Ok(json!({"groups": [{"id": "g1", "name": group_name}]}));
            }
            Ok(json!({}))
        })),
        ..Default::default()
    };
    let (status, text) = call(&app, "DELETE", &format!("/api/tasks/{id}"), None);
    assert_eq!(status, 200, "delete status = {status} body = {text}");
    let got = std::fs::read_to_string(&log).unwrap();
    for want in [
        "terminal close --terminal term_plan --tab --json",
        "worktree rm --worktree branch:feat/GE-12-ship_billing-docs --json",
        "project setup-delete --setup setup_doc --force --json",
    ] {
        assert!(got.contains(want), "log missing {want:?}\n{got}");
    }
    assert_eq!(
        methods.lock().unwrap().join(","),
        "projectGroup.list,projectGroup.delete"
    );
    let left = app.store.tasks().unwrap();
    assert!(left.is_empty(), "left = {left:?}");

    let created = create_task(
        &app,
        r#"{"name":"GE-9 Keep","description":"Stay.","agents":[{"agentId":"requirement_planner"}]}"#,
    );
    app.store.set_task_branch(&created.id, "keep_me", "GE-9 Keep").unwrap();
    app.store
        .save_task_run(&created.id, "requirement_planner", "term_keep")
        .unwrap();
    let keep = created.id.clone();
    let fail_dir = tempfile::tempdir().unwrap();
    let fail = fail_dir.path().join("fail-orca");
    write_script(
        &fail,
        r#"#!/bin/sh
if [ "$1" = "status" ]; then
  printf '%s\n' '{"ok":true,"result":{"app":{"running":true}}}'
  exit 0
fi
printf '%s\n' '{"ok":false,"error":{"code":"runtime_error","message":"still open"}}'
"#,
    );
    app.orca = orca::Client {
        bin: fail.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let (status, text) = call(&app, "DELETE", &format!("/api/tasks/{keep}"), None);
    assert_eq!(status, 502, "failed delete status = {status} body = {text}");
    let kept = app.store.task(&created.id).unwrap();
    assert!(kept.started && kept.name == "GE-9 Keep", "kept = {kept:?}");
}

#[test]
fn task_belongs_to_group() {
    let (app, _tmp) = new_task_test_server();
    let snapshot = state(&app);
    assert_eq!(
        snapshot["groups"].as_array().map(Vec::len),
        Some(0),
        "groups = {:?}",
        snapshot["groups"]
    );
    let group = app
        .store
        .create_group(store::GroupInput {
            name: "Billing".into(),
            epic_key: "KIKI-8".into(),
            epic_board_id: "12".into(),
            epic_board_name: "KIKI".into(),
            ..Default::default()
        })
        .unwrap();
    let created = create_task(
        &app,
        &json!({"name": "Invoice page", "description": "Add it.", "groupId": group.id}).to_string(),
    );
    assert!(
        created.group_id == group.id && created.epic_key == "KIKI-8" && !created.epic_owner,
        "created = {created:?}"
    );
    let (status, text) = call(
        &app,
        "POST",
        "/api/groups",
        Some(r#"{"name":"Accounts","description":"No Jira."}"#),
    );
    assert_eq!(status, 201, "group status = {status} body = {text}");
    #[derive(Deserialize)]
    struct Made {
        group: store::Group,
    }
    let made = serde_json::from_str::<Made>(&text).unwrap();
    assert!(
        made.group.name == "Accounts" && made.group.epic_key.is_empty() && made.group.description == "No Jira.",
        "group = {:?}",
        made.group
    );
}

#[test]
fn branch_slug_and_ticket() {
    assert_eq!(branch_slug("  `Add shopping cart.`\nignored"), "add_shopping_cart");
    assert_eq!(branch_slug("Thêm đồ"), "th_m_d");
    assert_eq!(jira_ticket("see ge-12 now"), "GE-12");
    assert_eq!(jira_ticket("no ticket"), "");
}
