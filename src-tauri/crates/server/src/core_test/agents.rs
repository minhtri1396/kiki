//! What an agent file adds to the launch prompt, and how agents are saved.

use super::*;

#[test]
fn agent_mcp_and_tool_notes_reach_the_prompt() {
    let app = new_test_server();
    ensure_agent_files(root_str(&app)).unwrap();
    let state = serve(&app, "GET", "/api/state", None);
    let catalog = state_list(&state, "mcps");
    assert!(catalog == "[]" && state.contains(r#""tools":[]"#), "state = {state}");
    let planner = agent_in_state(&state, "requirement_planner");
    assert!(
        state_has_use(&planner["mcps"], "atlassian_jira") && state_has_use(&planner["mcps"], "atlassian_confluence"),
        "requirement_planner mcps = {:?}",
        planner["mcps"]
    );
    app.store.save_atlassian(&atlassian_jira_only()).unwrap();
    let state = serve(&app, "GET", "/api/state", None);
    let mcps = state_list(&state, "mcps");
    assert!(
        mcps.contains(r#""id":"atlassian_jira","name":"Atlassian - Jira""#) && !mcps.contains("atlassian_confluence"),
        "jira-only mcps = {mcps}"
    );
    app.store.save_atlassian(&atlassian_both()).unwrap();
    let state = serve(&app, "GET", "/api/state", None);
    assert!(
        state.contains(r#""id":"atlassian_jira","name":"Atlassian - Jira""#)
            && state.contains(r#""id":"atlassian_confluence","name":"Atlassian - Confluence""#)
            && state.contains(r#""tools":[]"#),
        "configured mcps = {state}"
    );

    let raw = serve(&app, "GET", "/api/agents/requirement_document", None);
    let mut agent: Value = serde_json::from_str(&raw).unwrap();
    agent["mcps"] = json!([{"id": "atlassian", "description": "Use the saved Atlassian setup."}]);
    serve(
        &app,
        "PUT",
        "/api/agents/requirement_document",
        Some(&json_bytes(&agent)),
    );
    let legacy_preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    for want in [
        "- Atlassian - Jira",
        "- Atlassian - Confluence",
        "Use the saved Atlassian setup.",
    ] {
        assert!(legacy_preview.contains(want), "missing {want:?} in\n{legacy_preview}");
    }

    agent["mcps"] = json!([
        {"id": "atlassian_jira", "description": "Read the Jira ticket.\nWrite the comment."},
        {"id": "atlassian_confluence", "description": "Read the design page."},
    ]);
    agent["tools"] = json!([{"id": "github", "description": "Open the pull request."}]);
    serve(
        &app,
        "PUT",
        "/api/agents/requirement_document",
        Some(&json_bytes(&agent)),
    );
    let saved = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(
        saved.contains("Read the Jira ticket.")
            && saved.contains("Read the design page.")
            && saved.contains(r#""id":"github""#)
            && !saved.contains("Open the pull request."),
        "saved agent = {saved}"
    );

    let preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    for want in [
        "MCP servers for this agent:",
        "- Atlassian - Jira",
        "- Atlassian - Confluence",
        "Use this MCP server:",
        "Read the Jira ticket.",
        "Write the comment.",
        "Read the design page.",
    ] {
        assert!(preview.contains(want), "missing {want:?} in\n{preview}");
    }
    assert!(
        !preview.contains("Tools for this agent:") && !preview.contains("Open the pull request."),
        "a tool that Settings has not enabled was included:\n{preview}"
    );

    app.store.save_atlassian(&mcp::Atlassian::default()).unwrap();
    let preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    assert!(
        !preview.contains("MCP servers for this agent:") && !preview.contains("Read the Jira ticket."),
        "an MCP that Settings has not set up was included:\n{preview}"
    );
    let kept = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(
        kept.contains("Read the Jira ticket.")
            && kept.contains("Read the design page.")
            && kept.contains(r#""id":"github""#),
        "clearing Settings dropped the agent notes: {kept}"
    );
}

#[test]
fn agent_handoffs_reach_the_prompt() {
    let app = new_test_server();
    ensure_agent_files(root_str(&app)).unwrap();
    let raw = serve(&app, "GET", "/api/agents/requirement_document", None);
    let mut agent: Value = serde_json::from_str(&raw).unwrap();
    agent["worksWith"] = json!([{
        "id": "requirement_designer",
        "handoffs": [
            {"when": "The requirement files are ready.", "send": "Design the folder.", "reply": "The design path.", "maxLoops": 2},
            {"when": "The user changes the scope.", "send": "Update the design."},
        ],
    }]);
    serve(
        &app,
        "PUT",
        "/api/agents/requirement_document",
        Some(&json_bytes(&agent)),
    );
    let saved = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(
        saved.contains(r#""send":"Design the folder.","when":"The requirement files are ready.""#)
            && saved.contains(r#""send":"Update the design.","when":"The user changes the scope.""#),
        "saved handoffs are missing from the agent"
    );
    let preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    for want in [
        "### requirement_designer",
        "#### Message 1 to requirement_designer",
        "The requirement files are ready.",
        "Design the folder.",
        "#### Message 2 to requirement_designer",
        "The user changes the scope.",
        "Update the design.",
    ] {
        assert!(preview.contains(want), "missing {want:?} in\n{preview}");
    }
    assert!(
        preview.contains("ask requirement_designer to report back to you, agent id requirement_document")
            && preview.contains("Report back: no."),
        "report back choice is missing:\n{preview}"
    );
    assert!(
        saved.contains(r#""maxLoops":2"#)
            && saved.contains(r#""reply":"The design path.""#)
            && preview.contains("Allow at most 2 loops.")
            && preview.contains("The design path."),
        "reply and loop limit are missing:\n{preview}"
    );
    assert!(
        !preview.contains("Message 3 to"),
        "an empty message was written:\n{preview}"
    );
}

#[test]
fn agent_repository_guide_reaches_the_prompt() {
    let app = new_test_server();
    ensure_agent_files(root_str(&app)).unwrap();
    let raw = serve(&app, "GET", "/api/agents/requirement_document", None);
    let mut agent: Value = serde_json::from_str(&raw).unwrap();
    assert!(
        raw.contains(r#""guide":"AGENTS.md""#),
        "a repository without a guide did not report the default: {raw}"
    );
    agent["repos"] =
        json!([{"kind": "document", "guide": "RULES.md", "description": "Write the requirements.", "readonly": false}]);
    serve(
        &app,
        "PUT",
        "/api/agents/requirement_document",
        Some(&json_bytes(&agent)),
    );
    let saved = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(
        saved.contains(r#""guide":"RULES.md""#),
        "saved guide file is missing from the agent"
    );
    let preview = serve(&app, "POST", "/api/agents/requirement_document/preview", Some(b"{}"));
    assert!(
        preview.contains("RULES.md (read it and follow it") && preview.contains("Write the requirements."),
        "preview does not name the guide file:\n{preview}"
    );
}

#[test]
fn launch_wait_for_reaches_the_prompt() {
    let app = new_test_server();
    ensure_agent_files(root_str(&app)).unwrap();
    let plain = serve(
        &app,
        "POST",
        "/api/agents/requirement_designer/preview",
        Some(br#"{"note":"Add a cart."}"#),
    );
    assert!(
        !plain.contains("WAIT FOR A REQUEST."),
        "an agent turned on by the user was told to wait:\n{plain}"
    );
    let waiting = serve(
        &app,
        "POST",
        "/api/agents/requirement_designer/preview",
        Some(br#"{"note":"Add a cart.","waitFor":["requirement_document","no_such_agent","requirement_designer"]}"#),
    );
    for want in ["WAIT FOR A REQUEST.", "requirement_document (Document Writer)"] {
        assert!(waiting.contains(want), "missing {want:?} in\n{waiting}");
    }
    assert!(
        !waiting.contains("no_such_agent"),
        "an unknown agent was named:\n{waiting}"
    );
}

#[test]
fn agent_handoffs_keep_a_note_with_no_condition() {
    let app = new_test_server();
    ensure_agent_files(root_str(&app)).unwrap();
    let raw = serve(&app, "GET", "/api/agents/requirement_document", None);
    let mut agent: Value = serde_json::from_str(&raw).unwrap();
    agent["worksWith"] = json!([{"id": "requirement_designer", "handoffs": [{"when": "", "send": "Design it."}]}]);
    let (status, body) = call(&app, "PUT", "/api/agents/requirement_document", &agent.to_string());
    assert!(status == 200, "a note with no condition was rejected: {status} {body}");
    let saved = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(
        saved.contains("Design it.") && saved.contains(r#""when":"""#),
        "saved note = {saved}"
    );
    agent["worksWith"] =
        json!([{"id": "requirement_designer", "handoffs": [{"when": "The files are ready.", "send": "  "}]}]);
    let (status, body) = call(&app, "PUT", "/api/agents/requirement_document", &agent.to_string());
    assert!(
        status == 400 && body.contains("What to send"),
        "empty send: got {status} {body}"
    );
}

#[test]
fn create_and_delete_agent() {
    let app = new_test_server();
    ensure_agent_files(root_str(&app)).unwrap();
    let raw = serve(&app, "GET", "/api/agents/backend_runner", None);
    let mut agent: Value = serde_json::from_str(&raw).unwrap();
    agent["id"] = json!("ignored_by_the_server");
    agent["name"] = json!("Support Runner");
    agent["category"] = json!("Support Team");
    agent["description"] = json!("Answers support questions.");
    agent["worksWith"] = json!([]);
    let created = serve(&app, "POST", "/api/agents", Some(&json_bytes(&agent)));
    assert!(
        created.contains(r#""id":"support_team_support_runner""#) && created.contains(r#""title":"Support Runner""#),
        "created = {created}"
    );
    let again = call(&app, "POST", "/api/agents", &agent.to_string());
    assert!(
        again.0 == 400 && again.1.contains("already exists"),
        "duplicate = {} {}",
        again.0,
        again.1
    );
    agent["name"] = json!("!!!");
    let bad = call(&app, "POST", "/api/agents", &agent.to_string());
    assert!(
        bad.0 == 400 && bad.1.contains("enter a category and a name"),
        "bad name = {} {}",
        bad.0,
        bad.1
    );

    let removed = serve(&app, "DELETE", "/api/agents/support_team_support_runner", None);
    assert!(
        !removed.contains(r#""id":"support_team_support_runner""#),
        "removed = {removed}"
    );
    let missing = call(&app, "GET", "/api/agents/support_team_support_runner", "");
    assert_eq!(missing.0, 404, "missing = {}", missing.1);

    let document = serve(&app, "GET", "/api/agents/requirement_document", None);
    assert!(document.contains("requirement_designer"), "document = {document}");
    serve(&app, "DELETE", "/api/agents/requirement_designer", None);
    let document = serve(&app, "GET", "/api/agents/requirement_document", None);
    let saved: Value = serde_json::from_str(&document).unwrap();
    assert!(
        !saved["worksWith"].to_string().contains("requirement_designer"),
        "worksWith = {}",
        saved["worksWith"]
    );
    let gone = call(&app, "DELETE", "/api/agents/requirement_designer", "");
    assert_eq!(gone.0, 404, "gone = {}", gone.1);
}
