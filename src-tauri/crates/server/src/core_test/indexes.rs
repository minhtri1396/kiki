//! Repository indexes found in a folder or chosen by the user.

use super::*;

fn index_of(state: &Value, kind: &str) -> String {
    for row in state["repos"].as_array().unwrap() {
        if row["kind"] == kind {
            return row["index"].as_str().unwrap_or("").to_string();
        }
    }
    panic!("repo {kind} missing");
}

fn present_of(state: &Value, kind: &str) -> Vec<String> {
    for row in state["indexes"].as_array().unwrap() {
        if row["kind"] != kind {
            continue;
        }
        return row["present"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
    }
    panic!("index {kind} missing");
}

#[test]
fn remember_present_indexes_saves_the_folder_that_exists() {
    let app = new_test_server();
    app.store.ensure_layout(root_str(&app)).unwrap();
    let backend = app.root.join("backend");
    let docs = app.root.join("docs");
    let website = app.root.join("website");
    let mobile = app.root.join("mobile");
    for dir in [
        backend.join(".codegraph"),
        docs.join(".obsidian"),
        website.join(".codegraph"),
        website.join(".obsidian"),
        mobile.join(".codegraph"),
    ] {
        fs::create_dir(&dir).unwrap();
    }
    let snap = app.load().unwrap();
    app.store.save_repo_index(&snap, "mobile", "obsidian").unwrap();
    let snap = app.load().unwrap();
    let snap = app.remember_present_indexes(snap).unwrap();
    assert_eq!(
        snap.repo("backend").index,
        "codegraph",
        "backend = {:?}",
        snap.repo("backend").index
    );
    assert_eq!(
        snap.repo("document").index,
        "obsidian",
        "document = {:?}",
        snap.repo("document").index
    );
    assert_eq!(
        snap.repo("website").index,
        "",
        "website = {:?}",
        snap.repo("website").index
    );
    assert_eq!(
        snap.repo("mobile").index,
        "obsidian",
        "mobile = {:?}",
        snap.repo("mobile").index
    );

    let saved = app.load().unwrap();
    assert!(
        saved.repo("backend").index == "codegraph"
            && saved.repo("document").index == "obsidian"
            && saved.repo("website").index.is_empty()
            && saved.repo("mobile").index == "obsidian",
        "saved backend={:?} document={:?} website={:?} mobile={:?}",
        saved.repo("backend").index,
        saved.repo("document").index,
        saved.repo("website").index,
        saved.repo("mobile").index
    );
}

#[test]
fn use_index_keeps_an_existing_folder() {
    let app = new_test_server();
    app.store.ensure_layout(root_str(&app)).unwrap();
    fs::create_dir(app.root.join("backend").join(".codegraph")).unwrap();
    let (status, body) = call(&app, "PUT", "/api/indexes/backend", r#"{"index":"codegraph"}"#);
    assert_eq!(status, 200, "use status {status} body {body}");
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        index_of(&state, "backend"),
        "codegraph",
        "backend index = {:?}",
        index_of(&state, "backend")
    );
    let present = present_of(&state, "backend");
    assert!(present.len() == 1 && present[0] == "codegraph", "present = {present:?}");

    let (status, body) = call(&app, "PUT", "/api/indexes/backend", r#"{"index":"obsidian"}"#);
    assert_eq!(status, 400, "missing folder status {status} body {body}");
    let snap = app.load().unwrap();
    assert_eq!(
        snap.repo("backend").index,
        "codegraph",
        "backend changed to {:?}",
        snap.repo("backend").index
    );
}

#[test]
fn setup_keeps_an_existing_codegraph_folder() {
    let app = new_test_server();
    app.store.ensure_layout(root_str(&app)).unwrap();
    fs::create_dir(app.root.join("backend").join(".codegraph")).unwrap();
    let (status, body) = call(&app, "POST", "/api/indexes/codegraph/backend", "");
    assert_eq!(status, 200, "status {status} body {body}");
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        index_of(&state, "backend"),
        "codegraph",
        "backend index = {:?}",
        index_of(&state, "backend")
    );
    let jobs = state["jobs"].as_array().cloned().unwrap_or_default();
    assert!(jobs.is_empty(), "setup started anyway: {body}");
}
