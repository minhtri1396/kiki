//! Copying a member's agents and repositories through the routes.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::*;

#[test]
fn team_member_replaces_agents() {
    const SHARED: &str = "---\nid: requirement_document\nname: From Team\ndescription: Shared copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - document\n---\n\nHello from the team folder.\n";
    const LOCAL: &str = "---\nid: backend_runner\nname: Local Runner\ndescription: Local copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - backend\n---\n\nLocal.\n";
    let app = new_team_test_server();
    let (d, team) = temp_team();
    let member = team.join("alice").join("agents").join("requirement");
    mkdirs(&member);
    write(&member.join("requirement_document.md"), SHARED);
    mkdirs(&team.join("bob").join("agents"));
    write(&team.join("bob").join("agents").join("old.md"), "old");
    let outside = d.path().join("outside");
    mkdirs(&outside);
    write(&outside.join("secret.md"), "secret");
    let agents = app.root.join(".kiki").join("agents").join("backend");
    mkdirs(&agents);
    write(&agents.join("backend_runner.md"), LOCAL);
    write(&app.root.join(".kiki").join("agents").join("notes.txt"), "notes");
    app.store
        .save_team_folder(&TeamFolder {
            path: s(&team).to_string(),
            user: "bob".into(),
            member: "bob".into(),
        })
        .unwrap();

    let req = post_json("/api/team/member", r#"{"name":"../outside"}"#);
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 400, "escape status = {status} body = {body}");
    assert!(exists(&app.root.join(".kiki/agents/backend/backend_runner.md")));

    let (status, body) = text(app.handle(&get("/api/team/members")));
    assert!(
        status == 200 && body.contains("\"alice\"") && !body.contains("outside"),
        "members status = {status} body = {body}"
    );

    let req = post_json("/api/team/member", r#"{"name":"alice"}"#);
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "status = {status} body = {body}");
    let payload: Value = serde_json::from_str(&body).unwrap();
    let tf = &payload["teamFolder"];
    assert!(
        tf["member"] == "alice" && tf["path"] == s(&team) && tf["stale"] == false && tf["userStale"] == true,
        "team = {tf}"
    );
    let agents_list = payload["agents"].as_array().unwrap();
    assert!(
        agents_list.len() == 1
            && agents_list[0]["id"] == "requirement_document"
            && agents_list[0]["title"] == "From Team",
        "agents = {agents_list:?}"
    );
    assert!(
        !exists(&app.root.join(".kiki/agents/backend/backend_runner.md")),
        "local agent file was kept"
    );
    assert!(
        !exists(&app.root.join(".kiki/agents/notes.txt")),
        "local notes file was kept"
    );
    let copied = app.root.join(".kiki/agents/requirement/requirement_document.md");
    assert_eq!(fs::read_to_string(&copied).unwrap(), SHARED);
    assert!(exists(&team.join("bob/agents/old.md")));
    assert!(
        !exists(&team.join("bob/agents/requirement/requirement_document.md")),
        "user agents folder was overwritten"
    );
    assert_eq!(
        fs::read_to_string(member.join("requirement_document.md")).unwrap(),
        SHARED
    );
    write(&copied, &format!("{SHARED}\nlocal edit\n"));
    assert!(
        team_stale(&app),
        ".kiki/agents differing from the member did not show sync"
    );
    write(&copied, SHARED);
    assert!(!team_stale(&app), "matching agents were reported as different");
    let updated = format!("{SHARED}\nUpdated by the member.\n");
    write(&member.join("requirement_document.md"), &updated);
    assert!(team_stale(&app), "member update was not reported");
    let req = post_json("/api/team/member", r#"{"name":"alice"}"#);
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "pull status = {status} body = {body}");
    assert!(
        !team_stale(&app) && team_flag(&app, "userStale"),
        "pull changed the user folder or left the member different"
    );
    assert!(exists(&team.join("bob/agents/old.md")));
    let req = post_json("/api/team/member", r#"{"name":"bob","direction":"push"}"#);
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "push status = {status} body = {body}");
    assert!(
        !(team_stale(&app) || team_flag(&app, "userStale")),
        "synced member is still different"
    );
    for path in [
        copied.clone(),
        team.join("bob/agents/requirement/requirement_document.md"),
    ] {
        let body = fs::read_to_string(&path).unwrap();
        assert_eq!(body, updated, "{}", path.display());
    }
    assert!(!exists(&team.join("bob/agents/old.md")), "user agent file was kept");

    let other_dir = tempfile::tempdir().unwrap();
    let other = other_dir.path().join("other-team");
    mkdirs(&other);
    let req = put_json("/api/team", &format!(r#"{{"path":"{}","user":"cara"}}"#, s(&other)));
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "save status = {status} body = {body}");
    let cfg = store_cfg(&app);
    assert!(
        cfg.path == s(&other) && cfg.user == "cara" && cfg.member == "cara",
        "changed folder = {cfg:?}"
    );
}

#[test]
fn team_member_requires_a_user() {
    let app = new_team_test_server();
    let (_d, team) = temp_team();
    let member = team.join("alice").join("agents");
    mkdirs(&member);
    write(&member.join("note.md"), "shared");
    let local = app.root.join(".kiki").join("agents");
    mkdirs(&local);
    write(&local.join("keep.md"), "keep");
    app.store
        .save_team_folder(&TeamFolder {
            path: s(&team).to_string(),
            ..Default::default()
        })
        .unwrap();
    let req = post_json("/api/team/member", r#"{"name":"alice"}"#);
    let (status, body) = text(app.handle(&req));
    assert!(
        status == 400 && body.contains("Settings"),
        "status = {status} body = {body}"
    );
    assert!(exists(&local.join("keep.md")));
}

#[test]
fn team_member_resolves_an_unknown_repository() {
    const SHARED: &str = "---\nid: requirement_document\nname: From Team\ndescription: Shared copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - id: payments\n    description: Take payments.\n---\n\nHello from the team folder.\n";
    let app = new_team_test_server();
    let (_d, team) = temp_team();
    let member = team.join("bob/agents/requirement");
    mkdirs(&member);
    write(&member.join("requirement_document.md"), SHARED);
    mkdirs(&app.root.join(".kiki/agents"));
    write(&app.root.join(".kiki/agents/keep.md"), "keep");
    app.store
        .save_team_folder(&TeamFolder {
            path: s(&team).to_string(),
            user: "alice".into(),
            member: "alice".into(),
        })
        .unwrap();

    let (status, body) = text(app.handle(&post_json("/api/team/member", r#"{"name":"bob"}"#)));
    assert!(
        status == 409 && body.contains("\"payments\""),
        "status = {status} body = {body}"
    );
    assert!(exists(&app.root.join(".kiki/agents/keep.md")));

    let req = post_json(
        "/api/team/member",
        r#"{"name":"bob","direction":"pull","repos":{"payments":{"action":"use","id":"backend"}}}"#,
    );
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "use status = {status} body = {body}");
    let got = fs::read_to_string(app.root.join(".kiki/agents/requirement/requirement_document.md")).unwrap();
    assert!(
        got.contains("id: backend") && !got.contains("id: payments"),
        "rewritten = {got}"
    );
    assert!(got.contains("Take payments."), "description was dropped:\n{got}");
    let source = fs::read_to_string(member.join("requirement_document.md")).unwrap();
    assert!(source.contains("id: payments"), "source member was rewritten: {source}");

    write(&member.join("requirement_document.md"), SHARED);
    let req = post_json(
        "/api/team/member",
        r#"{"name":"bob","direction":"pull","repos":{"payments":{"action":"create"}}}"#,
    );
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "create status = {status} body = {body}");
    let got = fs::read_to_string(app.root.join(".kiki/agents/requirement/requirement_document.md")).unwrap();
    assert!(got.contains("payments"), "created copy = {got}");
    assert!(exists(&app.root.join("payments")));
    let repo_file = fs::read_to_string(
        Path::new(app.store.file())
            .parent()
            .unwrap()
            .join("repos/payments.yaml"),
    )
    .unwrap();
    assert!(repo_file.contains("id: payments"), "repo file = {repo_file}");
    let snap = app.load().unwrap();
    assert_eq!(
        snap.repo("payments").folder,
        s(&app.root.join("payments")),
        "payments = {:?}",
        snap.repo("payments")
    );
}

#[test]
fn save_user_agents() {
    let app = new_team_test_server();
    let local_dir = app.root.join(".kiki/agents/backend");
    mkdirs(&local_dir);
    let local_path = local_dir.join("backend_runner.md");
    write(&local_path, LOCAL_AGENT_FILE);
    let team_dir = tempfile::tempdir().unwrap();
    let team = team_dir.path().to_path_buf();
    let team_file = team.join("alice/agents/backend/backend_runner.md");
    mkdirs(team_file.parent().unwrap());
    let team_body = LOCAL_AGENT_FILE.replacen("Local.", "From the team.", 1);
    write(&team_file, &team_body);
    write(&team.join("alice/agents/old.md"), "old");
    app.store
        .save_team_folder(&TeamFolder {
            path: s(&team).to_string(),
            user: "alice".into(),
            member: "alice".into(),
        })
        .unwrap();
    assert!(team_flag(&app, "userStale"), "different agents did not show save");
    let (status, body) = text(app.handle(&post_json("/api/team/agents", "")));
    assert_eq!(status, 200, "save status = {status} body = {body}");
    assert_eq!(fs::read_to_string(&team_file).unwrap(), LOCAL_AGENT_FILE);
    assert!(!exists(&team.join("alice/agents/old.md")), "old team file was kept");
    assert_eq!(fs::read_to_string(&local_path).unwrap(), LOCAL_AGENT_FILE);
    assert!(!team_flag(&app, "userStale"), "saved agents were still different");
    let cfg = store_cfg(&app);
    assert_eq!(cfg.member, "alice");
}

#[test]
fn team_member_push_overwrites_the_selected_folder() {
    let app = new_team_test_server();
    let local_dir = app.root.join(".kiki/agents/backend");
    mkdirs(&local_dir);
    write(&local_dir.join("backend_runner.md"), LOCAL_AGENT_FILE);
    let team_dir = tempfile::tempdir().unwrap();
    let team = team_dir.path().to_path_buf();
    mkdirs(&team.join("cara"));
    app.store
        .save_team_folder(&TeamFolder {
            path: s(&team).to_string(),
            user: "alice".into(),
            member: "alice".into(),
        })
        .unwrap();
    let req = post_json("/api/team/member", r#"{"name":"cara","direction":"pull"}"#);
    let (status, body) = text(app.handle(&req));
    assert!(
        status == 400 && body.contains("no agents folder"),
        "pull status = {status} body = {body}"
    );
    let req = post_json("/api/team/member", r#"{"name":"cara","direction":"sideways"}"#);
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 400, "direction status = {status} body = {body}");
    let req = post_json("/api/team/member", r#"{"name":"cara","direction":"push"}"#);
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "push status = {status} body = {body}");
    let got = fs::read_to_string(team.join("cara/agents/backend/backend_runner.md")).unwrap();
    assert_eq!(got, LOCAL_AGENT_FILE);
    let kept = fs::read_to_string(local_dir.join("backend_runner.md")).unwrap();
    assert_eq!(kept, LOCAL_AGENT_FILE);
    let cfg = store_cfg(&app);
    assert!(cfg.member == "cara" && cfg.user == "alice", "team = {cfg:?}");
    assert!(!team_stale(&app), "pushed agents were still different");
}

#[test]
fn team_repos_compare_copy_and_keep_the_checkout_folder() {
    let app = new_team_test_server();
    let checkout = app.root.join("backend");
    mkdirs(&checkout);
    let mut snap = app.load().unwrap();
    for repo in snap.repos.iter_mut() {
        if repo.kind != "backend" {
            continue;
        }
        repo.folder = s(&checkout).to_string();
        repo.services = vec!["api".to_string()];
        repo.index = "codegraph".to_string();
    }
    app.store.save(&snap).unwrap();
    let team_dir = tempfile::tempdir().unwrap();
    let team = team_dir.path().to_path_buf();
    app.store
        .save_team_folder(&TeamFolder {
            path: s(&team).to_string(),
            user: "alice".into(),
            member: "alice".into(),
        })
        .unwrap();
    let bob = team.join("bob/repos");
    mkdirs(&bob);
    let bob_file = "id: backend\nfolder: /other/machine/backend\nservices:\n  - web\nindex: codegraph\n";
    write(&bob.join("backend.yaml"), bob_file);

    let (status, body) = text(app.handle(&get("/api/team/compare?member=bob")));
    assert_eq!(status, 200, "compare status = {status} body = {body}");
    let compared: Value = serde_json::from_str(&body).unwrap();
    assert!(
        compared["member"]["reposDiffer"] == true
            && !compared["repos"].as_array().unwrap().is_empty()
            && compared["member"]["repos"].as_array().unwrap().len() == 1,
        "compare = {compared}"
    );

    let (status, body) = text(app.handle(&get("/api/team/diff?folder=bob&id=backend&part=repos")));
    assert_eq!(status, 200, "diff status = {status} body = {body}");
    assert!(
        !body.contains("/other/machine") && !body.contains(s(&checkout)),
        "diff includes a checkout folder: {body}"
    );
    assert!(body.contains("api") && body.contains("web"), "diff = {body}");

    let (status, body) = text(app.handle(&post_json("/api/team/repos", "")));
    assert_eq!(status, 200, "push status = {status} body = {body}");
    let pushed = fs::read_to_string(team.join("alice/repos/backend.yaml")).unwrap();
    assert!(
        !pushed.contains("folder:") && pushed.contains("api"),
        "pushed = {pushed}"
    );

    write(
        &team.join("alice/repos/backend.yaml"),
        "id: backend\nservices:\n  - billing\nindex: codegraph\n",
    );
    let req = post_json(
        "/api/team/member",
        r#"{"name":"alice","direction":"pull","part":"repos"}"#,
    );
    let (status, body) = text(app.handle(&req));
    assert_eq!(status, 200, "pull status = {status} body = {body}");
    let snap = app.load().unwrap();
    let got = snap.repo("backend");
    assert!(
        got.folder == s(&checkout) && got.services == vec!["billing".to_string()] && got.index == "codegraph",
        "backend = {got:?}"
    );
    let (status, body) = text(app.handle(&get("/api/team/compare?member=alice")));
    assert_eq!(status, 200, "after pull status = {status} body = {body}");
    let after: Value = serde_json::from_str(&body).unwrap();
    assert!(
        after["user"]["reposDiffer"] != true,
        "repositories still differ: {body}"
    );
}
