use std::sync::{Arc, Mutex};

use kiki_mcp as mcp;
use kiki_store as store;
use kiki_util::Ctx;
use serde::Deserialize;

use crate::tasks::test_support::{call, epic_group, new_task_test_server, state, task_test_server};

/// A Jira that records the URLs it is asked to delete and answers with the reply the test sets.
struct FakeJira {
    deleted: Mutex<Vec<String>>,
    reply: Mutex<mcp::HttpResponse>,
}

impl FakeJira {
    fn answering(status: u16, body: &[u8]) -> Arc<FakeJira> {
        Arc::new(FakeJira {
            deleted: Mutex::new(Vec::new()),
            reply: Mutex::new(mcp::HttpResponse {
                status,
                body: body.to_vec(),
            }),
        })
    }

    fn answer(&self, status: u16, body: &[u8]) {
        *self.reply.lock().unwrap() = mcp::HttpResponse {
            status,
            body: body.to_vec(),
        };
    }
}

impl mcp::HttpTransport for FakeJira {
    fn send(&self, _ctx: &Ctx, request: &mcp::HttpRequest<'_>) -> anyhow::Result<mcp::HttpResponse> {
        assert_eq!(request.method, mcp::Method::Delete);
        self.deleted.lock().unwrap().push(request.url.to_string());
        Ok(self.reply.lock().unwrap().clone())
    }
}

fn jira_settings() -> mcp::Atlassian {
    mcp::Atlassian {
        enabled: true,
        username: "a@acme.co".into(),
        api_token: "secret-jira".into(),
        jira: mcp::Service {
            enabled: true,
            url: "https://acme.atlassian.net".into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn delete_group_keeps_task_on_the_board() {
    let (app, _tmp) = new_task_test_server();
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
    app.store
        .create_task(store::TaskInput {
            name: "Invoice page".into(),
            group_id: group.id.clone(),
            ..Default::default()
        })
        .unwrap();
    let (status, text) = call(&app, "DELETE", &format!("/api/groups/{}", group.id), None);
    assert_eq!(status, 200, "status = {status} body = {text}");
    let snapshot = state(&app);
    let groups = snapshot["groups"].as_array().unwrap();
    assert!(
        groups.len() == 1
            && groups[0]["deleted"] == true
            && groups[0]["name"] == "Billing"
            && groups[0]["epicKey"] == "KIKI-8",
        "groups = {groups:?}"
    );
    let tasks = snapshot["tasks"].as_array().unwrap();
    assert!(
        tasks.len() == 1 && tasks[0]["groupId"] == group.id.as_str() && tasks[0]["epicKey"] == "KIKI-8",
        "tasks = {tasks:?}"
    );
    let (status, text) = call(&app, "DELETE", "/api/groups/01ARZ3NDEKTSV4RRFFQ69G5FAV", None);
    assert_eq!(status, 404, "missing status = {status} body = {text}");
}

#[test]
fn delete_group_can_delete_the_jira_ticket() {
    let jira = FakeJira::answering(204, b"");
    let transport: Arc<dyn mcp::HttpTransport> = jira.clone();
    let (app, _tmp) = task_test_server(|builder| builder.atlassian(mcp::AtlassianApi::new(transport)));
    app.store.save_atlassian(&jira_settings()).unwrap();
    let group = epic_group(&app, "Billing", "KIKI-12");

    let (status, text) = call(
        &app,
        "DELETE",
        &format!("/api/groups/{}", group.id),
        Some(r#"{"jira":true}"#),
    );
    assert_eq!(status, 200, "status = {status} body = {text}");
    assert_eq!(
        *jira.deleted.lock().unwrap(),
        ["https://acme.atlassian.net/rest/api/3/issue/KIKI-12"]
    );
    let saved = app.store.group(&group.id).unwrap();
    assert!(saved.deleted, "group = {saved:?}");

    let other = epic_group(&app, "Accounts", "KIKI-13");
    jira.answer(
        400,
        br#"{"errorMessages":["The issue can not be deleted because it has sub-tasks."]}"#,
    );
    let (status, text) = call(
        &app,
        "DELETE",
        &format!("/api/groups/{}", other.id),
        Some(r#"{"jira":true}"#),
    );
    assert!(
        status == 400 && text.contains("sub-tasks"),
        "status = {status} body = {text}"
    );
    let kept = app.store.group(&other.id).unwrap();
    assert!(!kept.deleted, "group = {kept:?}");
}

#[test]
fn delete_group_without_jira_leaves_the_epic_alone() {
    let jira = FakeJira::answering(204, b"");
    let transport: Arc<dyn mcp::HttpTransport> = jira.clone();
    let (app, _tmp) = task_test_server(|builder| builder.atlassian(mcp::AtlassianApi::new(transport)));
    app.store.save_atlassian(&jira_settings()).unwrap();
    let group = epic_group(&app, "Billing", "KIKI-12");
    for body in [None, Some("  "), Some(r#"{"jira":false}"#)] {
        let (status, text) = call(&app, "DELETE", &format!("/api/groups/{}", group.id), body);
        assert_eq!(status, 200, "body {body:?}: status = {status} {text}");
    }
    assert!(jira.deleted.lock().unwrap().is_empty());
    let (status, text) = call(&app, "DELETE", &format!("/api/groups/{}", group.id), Some("{nope"));
    assert_eq!((status, text.trim()), (400, r#"{"error":"invalid data"}"#));
}

#[test]
fn create_group_validates_and_has_no_epic_without_jira() {
    let (app, _tmp) = new_task_test_server();
    let (status, text) = call(&app, "POST", "/api/groups", Some(r#"{"name":"  ","description":"x"}"#));
    assert_eq!((status, text.trim()), (400, r#"{"error":"Enter a group name."}"#));
    let (status, text) = call(
        &app,
        "POST",
        "/api/groups",
        Some(r#"{"name":" Accounts ","description":null}"#),
    );
    assert_eq!(status, 201, "status = {status} body = {text}");
    #[derive(Deserialize)]
    struct Made {
        group: store::Group,
    }
    let made: Made = serde_json::from_str(&text).unwrap();
    assert!(
        made.group.name == "Accounts" && made.group.epic_key.is_empty(),
        "group = {:?}",
        made.group
    );
}
