//! Atlassian in Settings and launches: saved tokens, the MCP install, and the Jira epic note.

use super::*;

#[test]
fn save_atlassian_hides_tokens() {
    let app = new_test_server();
    let body = r#"{
		"workspaceId":"kiki","orcaAgent":"cursor","repos":[],
		"atlassian":{
			"enabled":true,
			"username":"a@acme.co","apiToken":"secret-jira-token",
			"jira":{"enabled":true,"url":"https://acme.atlassian.net"},
			"confluence":{"enabled":true,"url":"https://acme.atlassian.net/wiki"}
		}
	}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", body);
    assert_eq!(status, 200, "status = {status} body = {out}");
    assert!(
        out.contains(r#""apiToken":"secret-jira-token""#),
        "state hid the token: {out}"
    );
    assert!(
        out.contains(r#""apiTokenSet":true"#) && out.contains(r#""url":"https://acme.atlassian.net/wiki""#),
        "state = {out}"
    );
    let setup = fs::read_to_string(app.store.file()).unwrap();
    assert!(
        !setup.contains("secret-jira-token"),
        "setup file contains the token:\n{setup}"
    );
    let again = r#"{
		"workspaceId":"kiki","orcaAgent":"cursor","repos":[],
		"atlassian":{
			"enabled":true,
			"username":"a@acme.co",
			"jira":{"enabled":true,"url":"https://acme.atlassian.net"},
			"confluence":{"enabled":true,"url":"https://acme.atlassian.net/wiki"}
		}
	}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", again);
    assert_eq!(status, 200, "status = {status} body = {out}");
    let cfg = app.store.atlassian().unwrap();
    assert!(
        cfg.api_token == "secret-jira-token" && cfg.jira.api_token.is_empty() && cfg.confluence.api_token.is_empty(),
        "saved = {cfg:?}"
    );
}

#[test]
fn save_atlassian_rejects_partial_site() {
    let app = new_test_server();
    let body = r#"{"workspaceId":"kiki","orcaAgent":"cursor","repos":[],"atlassian":{"enabled":true,"jira":{"enabled":true,"url":"https://acme.atlassian.net"}}}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", body);
    assert!(
        status == 400 && out.contains("Atlassian needs an email"),
        "status = {status} body = {out}"
    );
}

#[test]
fn launch_installs_atlassian_mcp_for_the_agent_tool() {
    let mut app = new_test_server();
    let home = tempfile::tempdir().unwrap();
    app.mcp_home = home.path().to_str().unwrap().to_string();
    app.store.save_atlassian(&atlassian_both()).unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let err = app
        .launch_orchestrator(
            &Ctx::background(),
            &LaunchPrep {
                agent: agentdef::Agent {
                    mcps: vec![use_("atlassian_jira"), use_("atlassian_confluence")],
                    ..Default::default()
                },
                snap: console::Snapshot {
                    orchestrator: "other".into(),
                    workspace_id: "kiki".into(),
                    ..Default::default()
                },
                tool: AiTool::Cursor,
                worktree: worktree.path().to_str().unwrap().to_string(),
                ..Default::default()
            },
        )
        .expect_err("expected launch to stop");
    let path = home.path().join(".cursor").join("mcp.json");
    let err = format!("{err:#}");
    if let Ok(raw) = fs::read_to_string(&path) {
        assert!(
            raw.contains("secret-jira-token") && raw.contains(r#""mcp-atlassian-kiki""#),
            "mcp.json =\n{raw}"
        );
        assert!(
            err.contains("Agent Orchestration tool is not supported"),
            "launch err = {err}"
        );
        return;
    }
    assert!(err.contains("uvx is not installed"), "launch err = {err}");
}

#[test]
fn launch_stops_when_the_selected_mcp_is_not_set_up() {
    let mut app = new_test_server();
    let home = tempfile::tempdir().unwrap();
    app.mcp_home = home.path().to_str().unwrap().to_string();
    app.store.save_atlassian(&atlassian_jira_only()).unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let err = app
        .launch_orchestrator(
            &Ctx::background(),
            &LaunchPrep {
                agent: agentdef::Agent {
                    mcps: vec![use_("atlassian_confluence")],
                    ..Default::default()
                },
                snap: console::Snapshot {
                    orchestrator: "other".into(),
                    workspace_id: "kiki".into(),
                    ..Default::default()
                },
                tool: AiTool::Cursor,
                worktree: worktree.path().to_str().unwrap().to_string(),
                ..Default::default()
            },
        )
        .expect_err("expected launch to stop");
    assert!(
        format!("{err:#}").contains("Atlassian - Confluence"),
        "launch err = {err:#}"
    );
}

#[test]
fn launch_skips_mcp_the_agent_does_not_use() {
    let mut app = new_test_server();
    let home = tempfile::tempdir().unwrap();
    app.mcp_home = home.path().to_str().unwrap().to_string();
    app.store.save_atlassian(&atlassian_jira_only()).unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let err = app
        .launch_orchestrator(
            &Ctx::background(),
            &LaunchPrep {
                snap: console::Snapshot {
                    orchestrator: "other".into(),
                    workspace_id: "kiki".into(),
                    ..Default::default()
                },
                tool: AiTool::Cursor,
                worktree: worktree.path().to_str().unwrap().to_string(),
                ..Default::default()
            },
        )
        .expect_err("expected launch to stop");
    assert!(
        format!("{err:#}").contains("Agent Orchestration tool is not supported"),
        "launch err = {err:#}"
    );
    assert!(
        !home.path().join(".cursor").join("mcp.json").exists(),
        "wrote Cursor MCP settings for an agent that does not use Atlassian"
    );
}

// ---- epic_test.go ----

#[test]
fn epic_note_matches_the_agent_board() {
    let task = store::Task {
        epic_key: "KIKI-8".into(),
        epic_board_id: "12".into(),
        epic_board_name: "KIKI".into(),
        ..Default::default()
    };
    let note = "Task: Ship billing\n\nAdd the invoice page.";
    let jira = agentdef::Agent {
        mcps: vec![agentdef::Use {
            id: "atlassian_jira".into(),
            board_id: "12".into(),
            board_name: "KIKI".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let got = crate::epic::epic_note(note, &task, &jira);
    for want in [
        "Task: Ship billing",
        "Add the invoice page.",
        "Jira epic: KIKI-8",
        "Board: KIKI (12)",
        "This task belongs to this Jira epic. Do the work under this epic.",
        "If you create Jira tickets that are not sub-tasks, create them under this epic.",
        "If you create sub-tasks, create those sub-task tickets as children of the ticket this agent needs.",
    ] {
        assert!(got.contains(want), "missing {want:?} in\n{got}");
    }
    let other = agentdef::Agent {
        mcps: vec![agentdef::Use {
            id: "atlassian_jira".into(),
            board_id: "14".into(),
            board_name: "Platform".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        crate::epic::epic_note(note, &task, &other),
        note,
        "epic was sent to a different board"
    );
    let off = agentdef::Agent {
        mcps: vec![use_("github")],
        ..Default::default()
    };
    assert_eq!(
        crate::epic::epic_note(note, &task, &off),
        note,
        "epic was sent to an agent without Jira"
    );
    assert_eq!(
        crate::epic::epic_note(note, &store::Task::default(), &jira),
        note,
        "missing epic was sent"
    );
}
