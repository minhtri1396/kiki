use super::*;
use crate::parse::split;

use std::fs;
use std::path::Path;

#[test]
fn resolve_defaults_to_agents_beside_app() {
    let td = tmp();
    let root = s(td.path());
    let got = resolve(&root, "").unwrap();
    assert_eq!(got, s(&td.path().join(".kiki").join("agents")));
}

#[test]
fn resolve_rejects_relative_path() {
    let td = tmp();
    let root = s(td.path());
    assert!(
        resolve(&root, "agents").is_err(),
        "expected an error for a relative agent path"
    );
}

#[test]
fn ensure_creates_folder_and_agent_files() {
    let td = tmp();
    let dir = s(&td.path().join("agents"));
    ensure(&dir, "cursor").unwrap();
    let raw = fs::read_to_string(Path::new(&dir).join("requirement").join("requirement_document.md")).unwrap();
    let text = raw.as_str();
    for want in [
        "id: requirement_document\n",
        "name: Document Writer\n",
        "ai_tool: cursor\n",
        "model: auto\n",
        "enabled: true\n",
        "agents_coordination:\n",
        "id: requirement_designer\n",
        "description:",
        "tell the user to run agent id requirement_designer",
        "id: document\n",
        "business_requirement.md and product_requirement.md in this folder.",
        "# Agent: Requirement Document Writer",
    ] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
    let (front_matter, body) = split(text).expect("seeded document file has no front matter");
    assert!(
        !front_matter.contains("kind:")
            && !front_matter.contains("teamwork:")
            && !front_matter.contains("works_with:")
            && !front_matter.contains("collaboration:"),
        "seeded front matter still uses an old Agents Coordination key:\n{front_matter}"
    );
    assert!(
        !body.contains("## Works with agents")
            && !body.contains("**Agent ID:**")
            && !body.contains("**Document repository:**"),
        "seeded prompt repeats fields the launch prompt already includes"
    );
    let designer = fs::read_to_string(Path::new(&dir).join("requirement").join("requirement_designer.md")).unwrap();
    let (front, _) = split(&designer).expect("designer file has no front matter");
    assert!(
        front.contains("id: requirement_document\n") && front.contains("Do not message agent id requirement_document"),
        "designer front matter is missing the collaboration note:\n{front}"
    );
    let planner = find_ok(&dir, "requirement_planner");
    assert_eq!(
        planner.works_with.len(),
        5,
        "planner works with = {:?}",
        planner.works_with
    );
    for peer in &planner.works_with {
        assert!(
            !peer.handoffs.is_empty() && !peer.handoffs[0].send.trim().is_empty(),
            "planner peer {} has no message",
            peer.id
        );
    }
    assert_eq!(planner.repos.len(), 4, "planner repos = {:?}", planner.repos);
    for repo in &planner.repos {
        assert!(
            !repo.description.trim().is_empty(),
            "planner repo {} has no description",
            repo.kind
        );
    }
    assert!(
        planner.mcps.len() == 2
            && planner.mcps[0].id == "atlassian_jira"
            && planner.mcps[1].id == "atlassian_confluence"
            && planner.mcps[0]
                .description
                .contains("Do not call Jira until the user gives a requirement folder.")
            && planner.mcps[1]
                .description
                .contains("Do not create, edit, or delete a page."),
        "planner mcps = {:?}",
        planner.mcps
    );
    let document = find_ok(&dir, "requirement_document");
    assert!(
        document.mcps.is_empty() && document.tools.is_empty(),
        "document uses = {:?} {:?}",
        document.mcps,
        document.tools
    );
    let runner = find_ok(&dir, "backend_runner");
    assert_eq!(runner.works_with.len(), 1);
    assert!(
        runner.works_with[0].id == "backend_reviewer"
            && runner.works_with[0].handoffs.len() == 1
            && runner.works_with[0].handoffs[0]
                .send
                .contains("[backend_runner -> backend_reviewer]"),
        "runner collaboration = {:?}",
        runner.works_with[0]
    );
    assert!(
        runner.mcps.len() == 2
            && runner.mcps[0].id == "atlassian_jira"
            && runner.mcps[1].id == "atlassian_confluence"
            && runner.mcps[0].description.contains("Do not search Jira for work.")
            && runner.mcps[1]
                .description
                .contains("when the ticket or the user names it"),
        "runner mcps = {:?}",
        runner.mcps
    );
    let reviewer = find_ok(&dir, "backend_reviewer");
    assert!(
        reviewer.mcps.len() == 2
            && reviewer.mcps[0].id == "atlassian_jira"
            && reviewer.mcps[1].id == "atlassian_confluence"
            && reviewer.mcps[0].description.contains("Do not set a ticket to Done.")
            && reviewer.mcps[1]
                .description
                .contains("when the ticket, the pull request, or the user names it"),
        "reviewer mcps = {:?}",
        reviewer.mcps
    );
    assert!(
        front.contains("ai_tool: cursor\n") && front.contains("model: auto\n"),
        "designer did not use the default tool and model:\n{front}"
    );
}

#[test]
fn ensure_moves_flat_file_and_leaves_prompt() {
    let td = tmp();
    let dir = agents_dir(&td);
    fs::create_dir_all(&dir).unwrap();
    let flat = Path::new(&dir).join("backend_runner.md");
    let body = "---\nid: backend_runner\nname: Senior Backend Developer\ndescription: Implements a backend ticket.\nai_tool: cursor\nmodel: auto\nrepos:\n    - backend\nworks_with: backend_reviewer\n---\n\nCustom prompt line.\n";
    fs::write(&flat, body).unwrap();
    ensure(&dir, "cursor").unwrap();
    assert!(
        !flat.exists(),
        "flat agent file was left at the top of the agent folder"
    );
    let moved = fs::read_to_string(Path::new(&dir).join("backend").join("backend_runner.md")).unwrap();
    assert_eq!(moved, body, "moving into the category folder changed the file");
    let files = list(&dir).unwrap();
    assert!(
        files.len() == 1 && files[0].category == "backend" && files[0].id == "backend_runner",
        "files = {files:?}"
    );
    assert!(
        files[0].works_with.len() == 1
            && files[0].works_with[0].id == "backend_reviewer"
            && files[0].works_with[0].handoffs.is_empty(),
        "works with = {:?}",
        files[0].works_with
    );
    assert!(
        !Path::new(&dir)
            .join("requirement")
            .join("requirement_document.md")
            .exists(),
        "Ensure created a catalog agent after the folder already had an agent file"
    );
}
