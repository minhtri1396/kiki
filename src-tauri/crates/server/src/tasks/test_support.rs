//! Helpers the task, run and group tests share.

use std::path::Path;
use std::sync::Arc;

use kiki_agentfile as agentfile;
use kiki_jobs::Manager;
use kiki_orca as orca;
use kiki_store as store;
use serde::Deserialize;

use crate::http::Request;
use crate::{Server, ServerBuilder};

/// A server over a temp folder with the default agent files. The temp folder lives as long as the returned guard.
pub(crate) fn new_task_test_server() -> (Server, tempfile::TempDir) {
    task_test_server(|builder| builder)
}

/// Like [`new_task_test_server`], with the builder adjusted by `adjust` (for example to inject a fake service).
pub(crate) fn task_test_server(adjust: impl FnOnce(ServerBuilder) -> ServerBuilder) -> (Server, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_string_lossy().into_owned();
    let db = store::open(&format!("{root}/.kiki/kiki.yaml")).unwrap();
    let dir = agentfile::resolve(&root, "").unwrap();
    agentfile::ensure(&dir, "cursor").unwrap();
    let home = format!("{root}/home");
    let builder = Server::builder(root, Arc::new(db), orca::Client::default(), Arc::new(Manager::new())).mcp_home(home);
    (adjust(builder).build(), tmp)
}

/// Sends a request through the router and returns `(status, body)`.
pub(crate) fn call(srv: &Server, method: &str, path: &str, body: Option<&str>) -> (u16, String) {
    let mut req = Request::new(method, path);
    if let Some(body) = body {
        req = req.with_body(body);
    }
    let resp = srv.handle(&req);
    (resp.status, resp.into_text())
}

pub(crate) fn write_script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[derive(Deserialize)]
pub(crate) struct TaskBody {
    pub task: store::Task,
}

pub(crate) fn create_task(srv: &Server, body: &str) -> store::Task {
    let (status, text) = call(srv, "POST", "/api/tasks", Some(body));
    assert_eq!(status, 201, "create status = {status} body = {text}");
    serde_json::from_str::<TaskBody>(&text).unwrap().task
}

/// The `/api/state` snapshot the web UI renders.
pub(crate) fn state(srv: &Server) -> serde_json::Value {
    let (status, text) = call(srv, "GET", "/api/state", None);
    assert_eq!(status, 200, "state = {text}");
    serde_json::from_str(&text).unwrap()
}

/// A group with a Jira epic, as the board tests need it.
pub(crate) fn epic_group(srv: &Server, name: &str, epic_key: &str) -> store::Group {
    srv.store
        .create_group(store::GroupInput {
            name: name.into(),
            epic_key: epic_key.into(),
            epic_board_id: "12".into(),
            epic_board_name: "KIKI".into(),
            ..Default::default()
        })
        .unwrap()
}
