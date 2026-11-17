//! Tests of the team folder routes (through `Server::handle`) and of the pieces they are built from.

mod browse;
mod folder;
mod member;
mod units;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::http::{Request, Response};
use crate::team::FolderPicker;
use crate::Server;
use kiki_jobs::Manager;
use kiki_orca::Client;
use kiki_store::{self as store, TeamFolder};

const LOCAL_AGENT_FILE: &str = "---\nid: backend_runner\nname: Local Runner\ndescription: Local copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - backend\n---\n\nLocal.\n";

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

fn new_team_test_server() -> TestApp {
    team_test_server(FolderPicker::system())
}

fn team_test_server(picker: FolderPicker) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let db = store::open(root.join("kiki.yaml").to_str().unwrap()).unwrap();
    let srv = Server::builder(
        root.to_str().unwrap(),
        Arc::new(db),
        Client {
            dir: root.to_str().unwrap().to_string(),
            ..Default::default()
        },
        Arc::new(Manager::new()),
    )
    .mcp_home(root.join("home").to_str().unwrap())
    .rtk_hooks(|_, _| Ok(()))
    .guard_hooks(|_, _| Ok(()))
    .folder_picker(picker)
    .build();
    TestApp { srv, root, _dir: dir }
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn mkdirs(p: &Path) {
    fs::create_dir_all(p).unwrap();
}

fn write(p: &Path, body: &str) {
    fs::write(p, body).unwrap();
}

fn exists(p: &Path) -> bool {
    p.exists()
}

fn temp_team() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let team = dir.path().join("team");
    (dir, team)
}

fn text(resp: Response) -> (u16, String) {
    let status = resp.status;
    (status, resp.into_text())
}

fn put_json(path: &str, body: &str) -> Request {
    Request::new("PUT", path).with_body(body.to_string())
}

fn post_json(path: &str, body: &str) -> Request {
    Request::new("POST", path).with_body(body.to_string())
}

fn get(path: &str) -> Request {
    Request::new("GET", path)
}

fn team_flag(app: &TestApp, name: &str) -> bool {
    let resp = app.handle(&get("/api/state"));
    let (status, body) = text(resp);
    assert_eq!(status, 200, "state body = {body}");
    let state: Value = serde_json::from_str(&body).unwrap();
    let team = state["teamFolder"]
        .as_object()
        .unwrap_or_else(|| panic!("team folder = {:?}", state["teamFolder"]));
    team[name]
        .as_bool()
        .unwrap_or_else(|| panic!("{name} = {:?}", team[name]))
}

fn team_stale(app: &TestApp) -> bool {
    team_flag(app, "stale")
}

fn store_cfg(app: &TestApp) -> TeamFolder {
    app.store.team_folder().unwrap()
}
