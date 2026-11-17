//! Settings tools and the hooks they install.

use super::*;

fn bool_json(on: bool) -> &'static str {
    if on {
        "true"
    } else {
        "false"
    }
}

#[test]
fn settings_tools_reach_the_agent_prompt() {
    let mut app = new_test_server();
    let hooks: Arc<Mutex<Vec<bool>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = hooks.clone();
    app.rtk_hooks = Some(Arc::new(move |home: &str, on: bool| {
        assert!(!home.is_empty(), "home is empty");
        seen.lock().unwrap().push(on);
        Ok(())
    }));
    ensure_agent_files(root_str(&app)).unwrap();
    let state = serve(&app, "GET", "/api/state", None);
    assert!(
        state.contains(r#""tools":[]"#)
            && state.contains(r#""id":"github_cli""#)
            && state.contains(r#""id":"rtk""#)
            && state.contains(r#""global":true"#),
        "state = {state}"
    );

    let save_tools = |github: bool, rtk: bool| -> String {
        let body = format!(
            r#"{{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{{"github_cli":{},"rtk":{}}}}}"#,
            bool_json(github),
            bool_json(rtk)
        );
        let (status, out) = call(&app, "PUT", "/api/setup", &body);
        assert_eq!(status, 200, "status = {status} body = {out}");
        out
    };
    let saved = save_tools(true, true);
    assert!(
        saved.contains(r#""tools":[{"id":"github_cli","name":"GitHub CLI"}]"#)
            && saved.contains(r#""id":"rtk""#)
            && saved.contains(r#""global":true"#),
        "enabled tools = {saved}"
    );
    let setup = fs::read_to_string(app.store.file()).unwrap();
    assert!(
        !setup.contains("github_cli") && !setup.contains("\nrtk:"),
        "setup file contains tool settings:\n{setup}"
    );

    let raw = serve(&app, "GET", "/api/agents/requirement_document", None);
    let mut agent: Value = serde_json::from_str(&raw).unwrap();
    agent["tools"] = json!([
        {"id": "github_cli", "description": "Open the pull request."},
        {"id": "rtk", "description": "Shorten the output."},
    ]);
    serve(
        &app,
        "PUT",
        "/api/agents/requirement_document",
        Some(&json_bytes(&agent)),
    );
    let kept = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(
        kept.contains(r#""id":"github_cli""#)
            && !kept.contains(r#""id":"rtk""#)
            && !kept.contains("Open the pull request.")
            && !kept.contains("Shorten the output."),
        "saved agent = {kept}"
    );
    let preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    for want in [
        "Tools for this agent:",
        "- GitHub CLI",
        "The gh command opens pull requests, issues, and checks. Use gh for that work.",
    ] {
        assert!(preview.contains(want), "missing {want:?} in\n{preview}");
    }
    assert!(
        !preview.contains("RTK is on"),
        "the launch prompt told the model to run RTK:\n{preview}"
    );
    {
        let hooks = hooks.lock().unwrap();
        assert!(hooks.len() == 1 && hooks[0], "hooks = {hooks:?}");
    }
    assert!(
        !preview.contains("Open the pull request.") && !preview.contains("Use this tool:"),
        "the prompt used an agent-written tool note:\n{preview}"
    );

    save_tools(true, false);
    {
        let hooks = hooks.lock().unwrap();
        assert!(hooks.len() == 2 && !hooks[1], "hooks after turning RTK off = {hooks:?}");
    }
    let preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    assert!(
        !preview.contains("RTK is on") && preview.contains("- GitHub CLI"),
        "RTK stayed in the prompt or GitHub CLI dropped:\n{preview}"
    );

    save_tools(false, false);
    let preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    assert!(
        !preview.contains("Tools for this agent:")
            && !preview.contains("Use gh for that work.")
            && !preview.contains("RTK is on"),
        "a tool that Settings turned off was included:\n{preview}"
    );
    let kept = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(
        kept.contains(r#""id":"github_cli""#),
        "turning Settings off dropped the tool: {kept}"
    );
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn start_checks_the_agent_tool_hooks() {
    let mut app = new_test_server();
    let home = tempfile::tempdir().unwrap();
    app.mcp_home = path_str(home.path()).to_string();
    app.rtk_hooks = Some(Arc::new(|_, _| panic!("rtk init ran when the hook was already set")));
    app.store
        .save_tools(&tools::Settings {
            rtk: true,
            ..Default::default()
        })
        .unwrap();
    fs::create_dir_all(home.path().join(".cursor")).unwrap();
    let body = r#"{"version":1,"hooks":{"preToolUse":[{"command":"rtk hook cursor","matcher":"Shell"}]}}"#;
    fs::write(home.path().join(".cursor").join("hooks.json"), body).unwrap();
    app.ensure_agent_hooks(AiTool::Cursor).unwrap();

    let mut off = new_test_server();
    let off_home = tempfile::tempdir().unwrap();
    off.mcp_home = path_str(off_home.path()).to_string();
    off.ensure_agent_hooks(AiTool::Gemini).unwrap();

    let mut missing = new_test_server();
    let missing_home = tempfile::tempdir().unwrap();
    missing.mcp_home = path_str(missing_home.path()).to_string();
    missing
        .store
        .save_tools(&tools::Settings {
            rtk: true,
            ..Default::default()
        })
        .unwrap();
    let err = missing
        .ensure_agent_hooks(AiTool::Claude)
        .expect_err("expected the missing hook to be reported");
    assert!(format!("{err:#}").contains("Claude RTK hook"), "err = {err:#}");

    let mut quiet = new_test_server();
    let quiet_home = tempfile::tempdir().unwrap();
    quiet.mcp_home = path_str(quiet_home.path()).to_string();
    quiet
        .store
        .save_tools(&tools::Settings {
            guardrails: Some(false),
            ..Default::default()
        })
        .unwrap();
    quiet.ensure_agent_hooks(AiTool::Cursor).unwrap();
    assert!(
        !quiet_home.path().join(".cursor").join("hooks.json").exists(),
        "a hook was written while guardrails and RTK were off"
    );
}

#[test]
fn save_syncs_guardrails_with_the_setting() {
    let mut app = new_test_server();
    let got: Arc<Mutex<Vec<bool>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = got.clone();
    app.guard_hooks = Some(Arc::new(move |home: &str, on: bool| {
        assert!(!home.is_empty(), "home is empty");
        seen.lock().unwrap().push(on);
        Ok(())
    }));
    let body = r#"{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{"github_cli":false,"rtk":false,"guardrails":false}}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", body);
    assert!(
        status == 200 && out.contains(r#""guardrails":false"#),
        "status = {status} body = {out}"
    );
    {
        let got = got.lock().unwrap();
        assert!(got.len() == 1 && !got[0], "guardrails = {got:?}");
    }
    let body = r#"{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{"github_cli":true,"rtk":false}}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", body);
    assert!(
        status == 200 && out.contains(r#""guardrails":false"#),
        "omitted guardrails changed the setting: {status} {out}"
    );
    let body = r#"{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{"github_cli":false,"rtk":false,"guardrails":true}}"#;
    let (status, out) = call(&app, "PUT", "/api/setup", body);
    assert!(
        status == 200 && out.contains(r#""guardrails":true"#),
        "status = {status} body = {out}"
    );
    let got = got.lock().unwrap();
    assert!(got.len() == 3 && got[2], "guardrails = {got:?}");
}
