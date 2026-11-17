//! Folder level routes: save, browse, members, compare, diff.

use std::fs;

use serde_json::Value;

use super::*;

#[test]
fn team_save_requires_a_name() {
    let app = new_team_test_server();
    let (_d, team) = temp_team();
    mkdirs(&team);
    let req = put_json("/api/team", &format!(r#"{{"path":"{}","user":"  "}}"#, s(&team)));
    let (status, body) = text(app.handle(&req));
    assert!(
        status == 400 && body.contains("Enter your name."),
        "status = {status} body = {body}"
    );
    let cfg = store_cfg(&app);
    assert!(cfg.path.is_empty() && cfg.user.is_empty(), "folder was saved: {cfg:?}");
}

#[test]
fn team_save_rejects_missing_folder() {
    let app = new_team_test_server();
    let req = put_json("/api/team", r#"{"path":"/missing/kiki-team"}"#);
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 400, "body = {body}");
}

#[test]
fn team_members_lists_an_unsaved_folder() {
    let app = new_team_test_server();
    let (_d, team) = temp_team();
    mkdirs(&team.join("alice"));
    let req = get(&format!(
        "/api/team/members?path={}",
        url::form_urlencoded::byte_serialize(s(&team).as_bytes()).collect::<String>()
    ));
    let (status, body) = text(app.handle(&req));
    assert!(
        status == 200 && body.contains("\"alice\""),
        "status = {status} body = {body}"
    );
    let cfg = store_cfg(&app);
    assert!(
        cfg.path.is_empty() && cfg.member.is_empty(),
        "folder was saved: {cfg:?}"
    );
}

#[test]
fn team_save_copies_agents_under_the_user() {
    let app = new_team_test_server();
    let agents = app.root.join(".kiki/agents/backend");
    mkdirs(&agents);
    write(&agents.join("backend_runner.md"), LOCAL_AGENT_FILE);
    let (_d, team) = temp_team();
    mkdirs(&team);
    let body = format!(r#"{{"path":"{}","user":"alice"}}"#, s(&team));
    let (status, body) = text(app.handle(&put_json("/api/team", &body)));
    assert_eq!(status, 200, "status = {status} body = {body}");
    let got = fs::read_to_string(team.join("alice/agents/backend/backend_runner.md")).unwrap();
    assert_eq!(got, LOCAL_AGENT_FILE);
    let cfg = store_cfg(&app);
    assert!(
        cfg.path == s(&team) && cfg.user == "alice" && cfg.member == "alice",
        "team = {cfg:?}"
    );
}

#[test]
fn team_members_reports_folders_that_differ() {
    let app = new_team_test_server();
    let agents = app.root.join(".kiki/agents");
    mkdirs(&agents);
    write(&agents.join("local.md"), "local\n");
    let (_d, team) = temp_team();
    mkdirs(&team.join("bob/agents"));
    write(&team.join("bob/agents/bob.md"), "bob\n");
    mkdirs(&team.join("cara"));
    let req = put_json("/api/team", &format!(r#"{{"path":"{}","user":"alice"}}"#, s(&team)));
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "save status = {status} body = {body}");

    let (status, body) = text(app.handle(&get("/api/team/members")));
    assert_eq!(status, 200, "status = {status} body = {body}");
    let payload: Value = serde_json::from_str(&body).unwrap();
    let differs: Vec<&str> = payload["differs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        differs.contains(&"bob") && differs.contains(&"cara") && !differs.contains(&"alice"),
        "differs = {differs:?}"
    );
}

#[test]
fn team_compare_lists_app_user_and_member() {
    let app = new_team_test_server();
    let local_dir = app.root.join(".kiki/agents/backend");
    mkdirs(&local_dir);
    write(&local_dir.join("backend_runner.md"), LOCAL_AGENT_FILE);
    let (_d, team) = temp_team();
    mkdirs(&team.join("bob/agents/backend"));
    let bob = LOCAL_AGENT_FILE.replacen("Local Runner", "Bob Runner", 1);
    write(&team.join("bob/agents/backend/backend_runner.md"), &bob);
    let req = put_json("/api/team", &format!(r#"{{"path":"{}","user":"alice"}}"#, s(&team)));
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "save status = {status} body = {body}");

    let (status, body) = text(app.handle(&get("/api/team/compare?member=bob")));
    assert_eq!(status, 200, "status = {status} body = {body}");
    let payload: Value = serde_json::from_str(&body).unwrap();
    let members: Vec<&str> = payload["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        members.contains(&"alice") && members.contains(&"bob"),
        "members = {members:?}"
    );
    let user = &payload["user"];
    let user_agents = user["agents"].as_array().unwrap();
    assert!(
        user["name"] == "alice"
            && user["differs"] == false
            && user_agents.len() == 1
            && user_agents[0]["name"] == "Local Runner",
        "user = {user}"
    );
    let member = &payload["member"];
    let member_agents = member["agents"].as_array().unwrap();
    assert!(
        member["name"] == "bob"
            && member["differs"] == true
            && member_agents.len() == 1
            && member_agents[0]["name"] == "Bob Runner",
        "member = {member}"
    );
    let app_agents = payload["app"].as_array().unwrap();
    assert!(
        app_agents.len() == 1 && app_agents[0]["name"] == "Local Runner",
        "app = {app_agents:?}"
    );
    let app_hash = app_agents[0]["hash"].as_str().unwrap();
    assert!(
        !app_hash.is_empty() && user_agents[0]["hash"] == app_hash && member_agents[0]["hash"] != app_hash,
        "hashes app={app_hash:?} user={} member={}",
        user_agents[0]["hash"],
        member_agents[0]["hash"]
    );

    let (status, body) = text(app.handle(&get("/api/team/diff?folder=bob&id=backend_runner")));
    assert_eq!(status, 200, "diff status = {status} body = {body}");
    let diff: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(diff["name"], "Local Runner");
    let lines = diff["diff"].as_array().unwrap();
    let saw_local = lines
        .iter()
        .any(|l| l["kind"] == "del" && l["text"].as_str().unwrap().contains("Local Runner"));
    let saw_bob = lines
        .iter()
        .any(|l| l["kind"] == "add" && l["text"].as_str().unwrap().contains("Bob Runner"));
    assert!(saw_local && saw_bob, "diff = {lines:?}");
}

#[test]
fn team_diff_shows_a_category_change() {
    let app = new_team_test_server();
    let local_dir = app.root.join(".kiki/agents/backend");
    mkdirs(&local_dir);
    write(&local_dir.join("backend_runner.md"), LOCAL_AGENT_FILE);
    let (_d, team) = temp_team();
    let moved = team.join("bob/agents/frontend");
    mkdirs(&moved);
    write(&moved.join("backend_runner.md"), LOCAL_AGENT_FILE);
    let req = put_json("/api/team", &format!(r#"{{"path":"{}","user":"alice"}}"#, s(&team)));
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "save status = {status} body = {body}");

    let (status, body) = text(app.handle(&get("/api/team/compare?member=bob")));
    assert_eq!(status, 200, "compare status = {status} body = {body}");
    let compared: Value = serde_json::from_str(&body).unwrap();
    let app_agents = compared["app"].as_array().unwrap();
    let member_agents = compared["member"]["agents"].as_array().unwrap();
    assert!(
        app_agents.len() == 1 && member_agents.len() == 1,
        "compare = {compared}"
    );
    assert!(
        app_agents[0]["category"] == "backend" && member_agents[0]["category"] == "frontend",
        "categories = {} {}",
        app_agents[0]["category"],
        member_agents[0]["category"]
    );
    assert!(
        compared["member"]["differs"] == true
            && !app_agents[0]["hash"].as_str().unwrap().is_empty()
            && app_agents[0]["hash"] == member_agents[0]["hash"],
        "member = {} app = {app_agents:?}",
        compared["member"]
    );

    let (status, body) = text(app.handle(&get("/api/team/diff?folder=bob&id=backend_runner")));
    assert_eq!(status, 200, "diff status = {status} body = {body}");
    let diff: Value = serde_json::from_str(&body).unwrap();
    let lines = diff["diff"].as_array().unwrap();
    let saw_backend = lines
        .iter()
        .any(|l| l["kind"] == "del" && l["text"] == "category: backend");
    let saw_frontend = lines
        .iter()
        .any(|l| l["kind"] == "add" && l["text"] == "category: frontend");
    assert!(saw_backend && saw_frontend, "diff = {lines:?}");
}

#[test]
fn team_save_asks_when_the_user_already_has_agents() {
    let app = new_team_test_server();
    mkdirs(&app.root.join(".kiki/agents/backend"));
    write(
        &app.root.join(".kiki/agents/backend/backend_runner.md"),
        LOCAL_AGENT_FILE,
    );
    let (_d, team) = temp_team();
    let shared = team.join("alice/agents/requirement");
    mkdirs(&shared);
    write(&shared.join("requirement_document.md"), "team\n");
    let req = put_json("/api/team", &format!(r#"{{"path":"{}","user":"alice"}}"#, s(&team)));
    let (status, body) = text(app.handle(&req));
    assert!(
        status == 409 && body.contains(r#""conflict":"name""#),
        "status = {status} body = {body}"
    );
    let cfg = store_cfg(&app);
    assert!(
        cfg.path.is_empty() && cfg.user.is_empty(),
        "folder was saved before a choice: {cfg:?}"
    );
    let req = put_json(
        "/api/team",
        &format!(r#"{{"path":"{}","user":"alice","useExisting":true}}"#, s(&team)),
    );
    let (status, body) = text(app.handle(&req));
    assert!(
        status == 409 && body.contains(r#""conflict":"agents""#),
        "own folder status = {status} body = {body}"
    );
    let cfg = store_cfg(&app);
    assert!(
        cfg.path.is_empty() && cfg.user.is_empty(),
        "team changed before a choice: {cfg:?}"
    );
    assert!(
        !exists(&team.join("alice/agents/backend/backend_runner.md")),
        "local agents were copied before a choice"
    );

    let req = put_json(
        "/api/team",
        &format!(
            r#"{{"path":"{}","user":"alice","useExisting":true,"direction":"pull"}}"#,
            s(&team)
        ),
    );
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "pull status = {status} body = {body}");
    assert!(exists(
        &app.root.join(".kiki/agents/requirement/requirement_document.md")
    ));
    assert!(
        !exists(&app.root.join(".kiki/agents/backend/backend_runner.md")),
        "local agent file was kept"
    );
    let cfg = store_cfg(&app);
    assert!(cfg.user == "alice" && cfg.member == "alice", "team = {cfg:?}");
}
