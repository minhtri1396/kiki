//! The folder dialog is injected: the route answers with what the picker returns.

use anyhow::anyhow;

use super::*;

#[test]
fn team_browse_returns_chosen_path() {
    let dir = tempfile::tempdir().unwrap();
    let dir_s = s(dir.path()).to_string();
    let chosen = format!("{dir_s}/");
    let app = team_test_server(FolderPicker::new(move || Ok(chosen.clone())));
    let (status, body) = text(app.handle(&post_json("/api/team/browse", "")));
    assert_eq!(status, 200, "body = {body}");
    let payload: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(payload["path"].as_str().unwrap(), dir_s);
}

#[test]
fn team_browse_reports_a_cancelled_dialog() {
    let app = team_test_server(FolderPicker::new(|| Err(anyhow!("Folder selection was cancelled."))));
    let (status, body) = text(app.handle(&post_json("/api/team/browse", "")));
    assert_eq!(
        (status, body.as_str()),
        (400, "{\"error\":\"Folder selection was cancelled.\"}\n")
    );
}

#[test]
fn team_browse_rejects_a_folder_that_is_gone() {
    let app = team_test_server(FolderPicker::new(|| Ok("/missing/kiki-team".to_string())));
    let (status, _) = text(app.handle(&post_json("/api/team/browse", "")));
    assert_eq!(status, 400);
}

#[test]
fn a_second_team_operation_is_refused_while_one_runs() {
    let app = new_team_test_server();
    let _running = app.team_mu.lock().unwrap();
    let (status, body) = text(app.handle(&get("/api/team/members")));
    assert_eq!(
        (status, body.as_str()),
        (409, "{\"error\":\"Team folder setup is already running.\"}\n")
    );
}
