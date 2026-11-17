//! Generate text and the saved orchestrator.

use super::*;

#[test]
fn generate_rejects_empty_text() {
    let app = new_test_server();
    let (status, body) = call(&app, "POST", "/api/generate", r#"{"kind":"prompt","text":"  "}"#);
    assert_eq!(status, 400, "status = {status} body = {body}");
    assert!(body.contains("Enter some text first."), "body = {body}");
}

#[test]
fn save_keeps_generate_choice() {
    let app = new_test_server();
    let body =
        r#"{"workspaceId":"kiki","orcaAgent":"cursor","generateTool":"claude","generateModel":"sonnet","repos":[]}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", body);
    assert_eq!(status, 200, "status = {status} body = {out}");
    assert!(
        out.contains(r#""generateTool":"claude""#) && out.contains(r#""generateModel":"sonnet""#),
        "state = {out}"
    );
    let snap = app.store.load().unwrap();
    assert!(
        snap.generate_tool == "claude" && snap.generate_model == "sonnet",
        "saved = {} {}",
        snap.generate_tool,
        snap.generate_model
    );
    assert!(
        snap.orchestrator == "orca" && out.contains(r#""orchestrator":"orca""#),
        "orchestrator = {} state = {out}",
        snap.orchestrator
    );
}

#[test]
fn save_rejects_unknown_orchestrator() {
    let app = new_test_server();
    let body = r#"{"workspaceId":"kiki","orcaAgent":"cursor","orchestrator":"other","repos":[]}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", body);
    assert!(
        status == 400 && out.contains("Agent Orchestration tool is not supported"),
        "status = {status} body = {out}"
    );
}

#[test]
fn orchestrator_launch_follows_the_saved_tool() {
    let app = new_test_server();
    let err = app
        .launch_orchestrator(
            &Ctx::background(),
            &LaunchPrep {
                snap: console::Snapshot {
                    orchestrator: "other".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .expect_err("expected an error");
    assert!(
        format!("{err:#}").contains("Agent Orchestration tool is not supported"),
        "launch err = {err:#}"
    );
    let worktree = tempfile::tempdir().unwrap();
    let (command, project) = app
        .preview_orchestrator(&LaunchPrep {
            snap: console::Snapshot {
                workspace_id: "kiki".into(),
                orchestrator: "orca".into(),
                ..Default::default()
            },
            kind: "document".into(),
            tool: AiTool::Cursor,
            worktree: worktree.path().to_str().unwrap().to_string(),
            agent: agentdef::Agent {
                id: "requirement_document".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    assert!(
        project == "kiki-document" && command.contains("\norca "),
        "command = {command:?} project = {project:?}"
    );
}
