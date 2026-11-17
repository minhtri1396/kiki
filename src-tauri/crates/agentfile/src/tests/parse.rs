use super::*;

use std::fs;

#[test]
fn parse_accepts_legacy_repo_kind() {
    let td = tmp();
    let agents = td.path().join("agents");
    let dir = agents.join("requirement");
    fs::create_dir_all(&dir).unwrap();
    let body = "---\nid: requirement_document\nname: Document Writer\ndescription: Writes files.\nai_tool: cursor\nmodel: auto\nrepos:\n    - kind: document\n      description: Write the requirement here.\n---\n\nPrompt.\n";
    fs::write(dir.join("requirement_document.md"), body).unwrap();
    let file = find_ok(&s(&agents), "requirement_document");
    assert!(
        file.repos.len() == 1
            && file.repos[0].kind == "document"
            && file.repos[0].description == "Write the requirement here.",
        "repos = {:?}",
        file.repos
    );
}

#[test]
fn parse_accepts_legacy_teamwork_keys() {
    let td = tmp();
    let agents = td.path().join("agents");
    let dir = agents.join("backend");
    fs::create_dir_all(&dir).unwrap();
    let body = "---\nid: backend_runner\nname: Senior Backend Developer\ndescription: Implements a backend ticket.\nai_tool: cursor\nmodel: auto\nrepos:\n    - backend\nworks_with:\n    - id: backend_reviewer\n      collaboration: Hand the pull request over.\n---\n\nPrompt.\n";
    fs::write(dir.join("backend_runner.md"), body).unwrap();
    let file = find_ok(&s(&agents), "backend_runner");
    assert!(
        file.works_with.len() == 1
            && file.works_with[0].id == "backend_reviewer"
            && file.works_with[0].handoffs
                == vec![Handoff {
                    send: "Hand the pull request over.".into(),
                    ..Default::default()
                }],
        "works with = {:?}",
        file.works_with
    );
}

#[test]
fn parse_accepts_previous_teamwork_key() {
    let td = tmp();
    let agents = td.path().join("agents");
    let dir = agents.join("backend");
    fs::create_dir_all(&dir).unwrap();
    let body = "---\nid: backend_runner\nname: Senior Backend Developer\ndescription: Implements a backend ticket.\nai_tool: cursor\nmodel: auto\nrepos:\n    - backend\nteamwork:\n    - id: backend_reviewer\n      description: Hand the pull request over.\n---\n\nPrompt.\n";
    fs::write(dir.join("backend_runner.md"), body).unwrap();
    let file = find_ok(&s(&agents), "backend_runner");
    assert!(
        file.works_with.len() == 1
            && file.works_with[0].id == "backend_reviewer"
            && file.works_with[0].handoffs
                == vec![Handoff {
                    send: "Hand the pull request over.".into(),
                    ..Default::default()
                }],
        "works with = {:?}",
        file.works_with
    );
}
