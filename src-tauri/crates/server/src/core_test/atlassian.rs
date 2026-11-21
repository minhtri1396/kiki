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

#[derive(Debug)]
struct Listed {
    url: String,
    user: String,
    token: String,
}

/// Records the Jira and Confluence calls a list makes and answers with one board and one space.
struct Listing {
    seen: Mutex<Vec<Listed>>,
}

impl mcp::HttpTransport for Listing {
    fn send(&self, _ctx: &Ctx, request: &mcp::HttpRequest<'_>) -> anyhow::Result<mcp::HttpResponse> {
        self.seen.lock().unwrap().push(Listed {
            url: request.url.to_string(),
            user: request.user.to_string(),
            token: request.token.to_string(),
        });
        let body: &[u8] = if request.url.contains("/wiki/") {
            br#"{"results":[{"id":"1","key":"ENG","name":"Engineering"}]}"#
        } else {
            br#"{"values":[{"id":12,"name":"KIKI"}]}"#
        };
        Ok(mcp::HttpResponse {
            status: 200,
            body: body.to_vec(),
        })
    }
}

fn listing_server() -> (TestApp, Arc<Listing>) {
    let listing = Arc::new(Listing {
        seen: Mutex::new(Vec::new()),
    });
    let mut app = new_test_server();
    app.atlassian = mcp::AtlassianApi::new(listing.clone());
    (app, listing)
}

#[test]
fn board_and_space_lists_use_the_form_before_save() {
    let (app, listing) = listing_server();
    let (status, out) = call(&app, "GET", "/api/atlassian/jira/boards", "");
    assert!(
        status == 400 && out.contains("turn on Jira"),
        "status = {status} body = {out}"
    );

    let jira = r#"{"enabled":true,"username":"a@acme.co","apiToken":"typed-token","jira":{"enabled":true,"url":"https://typed.example"},"confluence":{"enabled":true,"url":"https://typed.example/wiki"}}"#;
    let (status, out) = call(&app, "POST", "/api/atlassian/jira/boards?q=KI", jira);
    assert!(status == 200 && out.contains("KIKI"), "status = {status} body = {out}");
    let (status, out) = call(&app, "POST", "/api/atlassian/confluence/spaces?q=eng", jira);
    assert!(status == 200 && out.contains("ENG"), "status = {status} body = {out}");
    let seen = listing.seen.lock().unwrap();
    assert!(
        seen.len() == 2
            && seen[0].user == "a@acme.co"
            && seen[0].token == "typed-token"
            && seen[0].url.starts_with("https://typed.example/rest/agile/1.0/board")
            && seen[0].url.contains("name=KI")
            && seen[1].url.starts_with("https://typed.example/wiki/api/v2/spaces"),
        "seen = {seen:?}"
    );
    drop(seen);
    let saved = app.store.atlassian().unwrap();
    assert!(
        saved.api_token.is_empty() && saved.jira.url.is_empty(),
        "saved = {saved:?}"
    );

    app.store.save_atlassian(&atlassian_jira_only()).unwrap();
    let moved = r#"{"enabled":true,"username":"a@acme.co","jira":{"enabled":true,"url":"https://other.example"}}"#;
    let (status, out) = call(&app, "POST", "/api/atlassian/jira/boards", moved);
    assert!(status == 200 && out.contains("KIKI"), "status = {status} body = {out}");
    let seen = listing.seen.lock().unwrap();
    assert!(
        seen[2].token == "secret-jira-token" && seen[2].url.starts_with("https://other.example/rest/agile/1.0/board"),
        "seen = {seen:?}"
    );
    drop(seen);
    assert_eq!(app.store.atlassian().unwrap().jira.url, "https://acme.atlassian.net");

    let (status, out) = call(&app, "GET", "/api/atlassian/jira/boards", "");
    assert!(status == 200 && out.contains("KIKI"), "status = {status} body = {out}");
    let seen = listing.seen.lock().unwrap();
    assert!(
        seen[3].token == "secret-jira-token"
            && seen[3]
                .url
                .starts_with("https://acme.atlassian.net/rest/agile/1.0/board"),
        "seen = {seen:?}"
    );

    let (status, out) = call(&app, "POST", "/api/atlassian/jira/boards", "{");
    assert_eq!((status, out.trim()), (400, r#"{"error":"invalid data"}"#));
}
