//! Tests of the core API: agents, Atlassian, Generate text, indexes, opening pages, Settings tools. Each area has
//! its own module; this one holds the test server and the helpers they share.

mod agents;
mod atlassian;
mod generate;
mod indexes;
mod open;
mod settings_tools;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use kiki_agentdef as agentdef;
use kiki_aitool::AiTool;
use kiki_console as console;
use kiki_jobs::Manager;
use kiki_mcp as mcp;
use kiki_orca::Client;
use kiki_store as store;
use kiki_store::Store;
use kiki_tools as tools;
use kiki_util::Ctx;

use crate::http::Request;
use crate::{ensure_agent_files, LaunchPrep, Server};

/// A server over a temporary folder; the command hooks do nothing.
struct TestApp {
    srv: Server,
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for TestApp {
    type Target = Server;
    fn deref(&self) -> &Server {
        &self.srv
    }
}

impl std::ops::DerefMut for TestApp {
    fn deref_mut(&mut self) -> &mut Server {
        &mut self.srv
    }
}

fn new_test_server() -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let db = Store::open(root.join("kiki.yaml").to_str().unwrap()).unwrap();
    let mut srv = Server::new(
        root.to_str().unwrap(),
        Arc::new(db),
        Client {
            dir: root.to_str().unwrap().to_string(),
            ..Default::default()
        },
        Arc::new(Manager::new()),
    );
    srv.mcp_home = root.join("home").to_str().unwrap().to_string();
    srv.rtk_hooks = Some(Arc::new(|_, _| Ok(())));
    srv.guard_hooks = Some(Arc::new(|_, _| Ok(())));
    TestApp { srv, root, _dir: dir }
}

fn root_str(app: &TestApp) -> &str {
    app.root.to_str().unwrap()
}

/// Sends the request and fails unless the status is 200.
fn serve(app: &Server, method: &str, path: &str, body: Option<&[u8]>) -> String {
    let mut req = Request::new(method, path);
    if let Some(body) = body {
        req = req.with_body(body.to_vec());
    }
    let resp = app.handle(&req);
    let status = resp.status;
    let text = resp.into_text();
    assert_eq!(status, 200, "{method} {path} = {status} {text}");
    text
}

/// Sends the request and returns the status and body.
fn call(app: &Server, method: &str, path: &str, body: &str) -> (u16, String) {
    let resp = app.handle(&Request::new(method, path).with_body(body.to_string()));
    (resp.status, resp.into_text())
}

fn state_list(raw: &str, key: &str) -> String {
    let doc: Value = serde_json::from_str(raw).unwrap();
    match doc.get(key) {
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

fn agent_in_state(raw: &str, id: &str) -> Value {
    let doc: Value = serde_json::from_str(raw).unwrap();
    for agent in doc["agents"].as_array().unwrap() {
        if agent["id"] == id {
            return agent.clone();
        }
    }
    panic!("agent {id} missing from state");
}

fn state_has_use(raw: &Value, id: &str) -> bool {
    raw.as_array()
        .map(|items| items.iter().any(|row| row["id"] == id))
        .unwrap_or(false)
}

fn json_bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}

fn atlassian_jira_only() -> mcp::Atlassian {
    mcp::Atlassian {
        enabled: true,
        username: "a@acme.co".into(),
        api_token: "secret-jira-token".into(),
        jira: mcp::Service {
            enabled: true,
            url: "https://acme.atlassian.net".into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn atlassian_both() -> mcp::Atlassian {
    mcp::Atlassian {
        confluence: mcp::Service {
            enabled: true,
            url: "https://acme.atlassian.net/wiki".into(),
            ..Default::default()
        },
        ..atlassian_jira_only()
    }
}

fn use_(id: &str) -> agentdef::Use {
    agentdef::Use {
        id: id.into(),
        ..Default::default()
    }
}
