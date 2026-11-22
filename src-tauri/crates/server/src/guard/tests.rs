use std::fs;
use std::path::Path;
use std::sync::Arc;

use kiki_agentfile as agentfile;
use kiki_jobs::Manager;
use kiki_orca as orca;
use kiki_store as store;
use serde_json::{json, Value};

use super::use_ids;
use crate::http::Request;
use crate::{ensure_agent_files, tools_ready, Server};

/// A server whose hooks are fakes: no real hook is installed.
fn new_guard_test_server() -> (Server, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_string_lossy().into_owned();
    let db = store::open(&format!("{root}/kiki.yaml")).unwrap();
    let home = format!("{root}/home");
    let srv = Server::builder(
        root.clone(),
        Arc::new(db),
        orca::Client {
            dir: root,
            ..Default::default()
        },
        Arc::new(Manager::new()),
    )
    .mcp_home(home)
    .rtk_hooks(|_, _| Ok(()))
    .guard_hooks(|_, _| Ok(()))
    .build();
    (srv, tmp)
}

#[test]
fn tools_must_be_set_up_before_launch() {
    let err = tools_ready(&["preview".to_string()], &[]).unwrap_err();
    assert!(err.to_string().contains("preview"), "missing tool = {err}");
    tools_ready(&[], &[]).unwrap();
}

struct Answer {
    allow: bool,
    message: String,
}

fn ask_guard(app: &Server, cwd: &str, agent: &str, event: &str, input: &str) -> Answer {
    let body = json!({
        "cwd": cwd,
        "agent": agent,
        "event": event,
        "input": serde_json::from_str::<Value>(input).unwrap(),
    });
    let resp = app.handle(&Request::new("POST", "/api/guardrail").with_json(&body));
    let status = resp.status;
    let text = resp.into_text();
    assert_eq!(status, 200, "status = {status} body = {text}");
    let payload: Value = serde_json::from_str(&text).unwrap();
    Answer {
        allow: payload["allow"].as_bool().unwrap(),
        message: payload["message"].as_str().unwrap().to_string(),
    }
}

fn use_repo_folder(app: &Server, kind: &str) -> String {
    let name = if kind == "document" { "docs" } else { kind };
    let folder = Path::new(&app.root).join(name);
    fs::create_dir_all(&folder).unwrap();
    let folder = folder.to_string_lossy().into_owned();
    let snap = app.store.load().unwrap();
    app.store.save_repo_folder(&snap, kind, &folder).unwrap();
    folder
}

fn agent_files(app: &Server) -> (String, Vec<agentfile::File>) {
    app.store.ensure_layout(&app.root).unwrap();
    ensure_agent_files(&app.root).unwrap();
    let dir = agentfile::resolve(&app.root, "").unwrap();
    let files = agentfile::list(&dir).unwrap();
    (dir, files)
}

fn file_path_json(path: &str) -> String {
    json!({ "file_path": path }).to_string()
}

#[test]
fn guardrail_blocks_what_the_agent_did_not_select() {
    let (app, _tmp) = new_guard_test_server();
    let (dir, files) = agent_files(&app);
    let mut reader = agentfile::File::default();
    let mut other = agentfile::File::default();
    for mut file in files {
        match file.id.as_str() {
            "requirement_document" => {
                file.enabled = true;
                file.repos = vec![agentfile::Repository {
                    kind: "document".into(),
                    ..Default::default()
                }];
                file.mcps = Vec::new();
                file.tools = Vec::new();
                file.works_with = Vec::new();
                reader = file.clone();
            }
            "requirement_planner" => {
                file.enabled = false;
                file.repos = vec![agentfile::Repository {
                    kind: "document".into(),
                    ..Default::default()
                }];
                file.mcps = Vec::new();
                other = file.clone();
            }
            _ => file.enabled = false,
        }
        agentfile::save(&dir, &file.id, &file).unwrap_or_else(|e| panic!("save {}: {e:#}", file.id));
    }
    assert!(!reader.id.is_empty() && !other.id.is_empty(), "missing agents");
    let doc = use_repo_folder(&app, "document");
    let backend = use_repo_folder(&app, "backend");

    let jira = r#"{"server":"mcp-atlassian-kiki","tool_name":"jira_search"}"#;
    let blocked = ask_guard(&app, &doc, "", "beforeMCPExecution", jira);
    assert!(
        !blocked.allow && blocked.message.contains("mcp-atlassian-kiki"),
        "mcp = {}",
        blocked.message
    );
    let file = file_path_json(&format!("{backend}/main.go"));
    let outside = ask_guard(&app, &doc, "", "beforeReadFile", &file);
    assert!(
        !outside.allow && outside.message.contains(&backend),
        "repo = {}",
        outside.message
    );
    let peer = ask_guard(
        &app,
        &doc,
        "",
        "subagentStart",
        r#"{"subagent_type":"requirement_planner"}"#,
    );
    assert!(
        !peer.allow && peer.message.contains("requirement_planner"),
        "agent = {}",
        peer.message
    );

    reader.mcps = vec![agentfile::Use {
        id: "atlassian_jira".into(),
        ..Default::default()
    }];
    reader.works_with = vec![agentfile::Collaborator {
        id: "requirement_planner".into(),
        ..Default::default()
    }];
    agentfile::save(&dir, &reader.id, &reader).unwrap();
    let allowed = ask_guard(&app, &doc, "", "beforeMCPExecution", jira);
    assert!(allowed.allow, "jira = {}", allowed.message);
    let page = ask_guard(
        &app,
        &doc,
        "",
        "preToolUse",
        r#"{"tool_name":"mcp__atlassian__confluence_get_page"}"#,
    );
    assert!(!page.allow, "confluence was allowed");
    let mate = ask_guard(
        &app,
        &doc,
        "",
        "subagentStart",
        r#"{"subagent_type":"requirement_planner"}"#,
    );
    assert!(mate.allow, "planner = {}", mate.message);

    other.enabled = true;
    agentfile::save(&dir, &other.id, &other).unwrap();
    let ambiguous = ask_guard(&app, &doc, "", "beforeShellExecution", r#"{"command":"ls"}"#);
    assert!(
        !ambiguous.allow && ambiguous.message.contains("requirement_document"),
        "ambiguous = {}",
        ambiguous.message
    );
    app.guard_binder().bind(&reader.id, &doc, false, &[]);
    let started = ask_guard(&app, &doc, "", "beforeMCPExecution", jira);
    assert!(started.allow, "started = {}", started.message);
    let named = ask_guard(
        &app,
        &doc,
        &other.id,
        "beforeMCPExecution",
        r#"{"server":"mcp-atlassian-kiki"}"#,
    );
    assert!(!named.allow, "the other agent was allowed to use Jira");
}

#[test]
fn guardrail_blocks_writes_when_the_repository_is_readonly() {
    let (app, _tmp) = new_guard_test_server();
    let (dir, files) = agent_files(&app);
    let mut reader = agentfile::File::default();
    for mut file in files {
        if file.id == "requirement_document" {
            file.enabled = true;
            file.repos = vec![
                agentfile::Repository {
                    kind: "document".into(),
                    readonly: true,
                    ..Default::default()
                },
                agentfile::Repository {
                    kind: "backend".into(),
                    ..Default::default()
                },
            ];
            file.mcps = Vec::new();
            file.tools = Vec::new();
            file.works_with = Vec::new();
            reader = file.clone();
        } else {
            file.enabled = false;
        }
        agentfile::save(&dir, &file.id, &file).unwrap_or_else(|e| panic!("save {}: {e:#}", file.id));
    }
    assert!(!reader.id.is_empty(), "missing agent");
    let doc = use_repo_folder(&app, "document");
    let backend = use_repo_folder(&app, "backend");
    let note = file_path_json(&format!("{doc}/notes.md"));
    let read = ask_guard(&app, &doc, &reader.id, "beforeReadFile", &note);
    assert!(read.allow, "read = {}", read.message);
    let edit = ask_guard(
        &app,
        &doc,
        &reader.id,
        "preToolUse",
        &format!(r#"{{"tool_name":"Write","tool_input":{note}}}"#),
    );
    assert!(
        !edit.allow && edit.message.contains("readonly"),
        "edit = {}",
        edit.message
    );
    let status = ask_guard(
        &app,
        &doc,
        &reader.id,
        "beforeShellExecution",
        r#"{"command":"git status"}"#,
    );
    assert!(status.allow, "status = {}", status.message);
    let commit = ask_guard(
        &app,
        &doc,
        &reader.id,
        "beforeShellExecution",
        r#"{"command":"git commit -am msg"}"#,
    );
    assert!(
        !commit.allow && commit.message.contains("readonly"),
        "commit = {}",
        commit.message
    );
    let code = json!({ "path": format!("{backend}/main.go") }).to_string();
    let change = ask_guard(
        &app,
        &backend,
        &reader.id,
        "preToolUse",
        &format!(r#"{{"tool_name":"Edit","tool_input":{code}}}"#),
    );
    assert!(change.allow, "backend edit = {}", change.message);
}

fn write_guard_project(root: &str, rel: &str, name: &str) {
    let dir = Path::new(root).join(rel);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("project.json"), format!(r#"{{"name":"{name}"}}"#)).unwrap();
}

#[test]
fn guardrail_blocks_an_nx_project_the_repository_did_not_select() {
    let (app, _tmp) = new_guard_test_server();
    let (dir, files) = agent_files(&app);
    let mut runner = agentfile::File::default();
    for mut file in files {
        if file.id == "backend_runner" {
            file.enabled = true;
            file.repos = vec![agentfile::Repository {
                kind: "backend".into(),
                ..Default::default()
            }];
            file.mcps = Vec::new();
            file.tools = Vec::new();
            file.works_with = Vec::new();
            runner = file.clone();
        } else {
            file.enabled = false;
        }
        agentfile::save(&dir, &file.id, &file).unwrap_or_else(|e| panic!("save {}: {e:#}", file.id));
    }
    assert!(!runner.id.is_empty(), "missing agent");
    let backend = use_repo_folder(&app, "backend");
    let mut snap = app.load().unwrap();
    for repo in snap.repos.iter_mut() {
        if repo.kind == "backend" {
            repo.services = vec!["api".to_string()];
        }
    }
    app.store.save(&snap).unwrap();
    write_guard_project(&backend, "apps/api", "api");
    write_guard_project(&backend, "apps/billing", "billing");
    fs::write(Path::new(&backend).join("README.md"), "hi").unwrap();
    let api = file_path_json(&format!("{backend}/apps/api/main.go"));
    let billing = file_path_json(&format!("{backend}/apps/billing/main.go"));
    let readme = file_path_json(&format!("{backend}/README.md"));
    let got = ask_guard(&app, &backend, &runner.id, "beforeReadFile", &api);
    assert!(got.allow, "api = {}", got.message);
    let blocked = ask_guard(&app, &backend, &runner.id, "beforeReadFile", &billing);
    assert!(
        !blocked.allow && blocked.message.contains("service billing") && blocked.message.contains("api"),
        "billing = {}",
        blocked.message
    );
    let got = ask_guard(&app, &backend, &runner.id, "beforeReadFile", &readme);
    assert!(got.allow, "readme = {}", got.message);
}

#[test]
fn use_ids_skips_blank_ids() {
    let list = vec![
        agentfile::Use {
            id: " atlassian_jira ".into(),
            ..Default::default()
        },
        agentfile::Use {
            id: "  ".into(),
            ..Default::default()
        },
    ];
    assert_eq!(use_ids(&list), ["atlassian_jira"]);
}

#[test]
fn the_binder_keeps_the_newest_bindings_and_ignores_bad_paths() {
    let (app, _tmp) = new_guard_test_server();
    let binder = app.guard_binder();
    binder.bind("", "/work/a", false, &[]);
    binder.bind("agent", "relative/path", false, &[]);
    binder.bind("agent", "  ", false, &[]);
    assert!(app.guard.lock().unwrap().bindings.is_empty(), "nothing valid was bound");

    for n in 0..45 {
        binder.bind("agent", &format!("/work/./tree{n}/"), n % 2 == 0, &["api".to_string()]);
    }
    let state = app.guard.lock().unwrap();
    assert_eq!(state.bindings.len(), 40);
    let first = &state.bindings[0];
    assert_eq!(
        (first.worktree.as_str(), first.read_only, first.services.clone()),
        ("/work/tree5", false, vec!["api".to_string()])
    );
    assert_eq!(state.bindings[39].worktree, "/work/tree44");
}

#[test]
fn the_binder_publishes_worktrees_once_the_app_has_an_address() {
    let (app, tmp) = new_guard_test_server();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let home_text = home.to_string_lossy().into_owned();
    {
        let mut state = app.guard.lock().unwrap();
        state.home = home_text.clone();
        state.url = "http://127.0.0.1:7777".to_string();
    }
    app.store.ensure_layout(&app.root).unwrap();
    app.guard_binder().bind("agent", "/work/tree", false, &[]);
    let published = kiki_guard::load(&home).unwrap();
    assert_eq!(published.len(), 1, "published = {published:?}");
    assert_eq!(published[0].url, "http://127.0.0.1:7777");
    assert!(
        published[0].paths.contains(&"/work/tree".to_string()),
        "paths = {:?}",
        published[0].paths
    );
}
