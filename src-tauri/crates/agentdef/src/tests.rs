//! Catalog and document checks.

use crate::*;

#[test]
fn embedded_documents_match_the_launch_prompt() {
    for agent in CATALOG.iter() {
        let body = agent.body().unwrap_or_else(|e| panic!("{}: {e:#}", agent.id));
        assert!(
            !body.contains("WAIT FOR A REQUEST."),
            "{} repeats the launch prompt's wait instruction",
            agent.id
        );
    }
}

#[test]
fn catalog_ids_are_unique_and_ordered_for_the_console() {
    let ids: Vec<&str> = CATALOG.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "requirement_document",
            "requirement_designer",
            "requirement_planner",
            "backend_runner",
            "backend_reviewer",
            "website_runner",
            "website_reviewer",
            "mobile_runner",
            "mobile_reviewer",
        ]
    );
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
}

#[test]
fn runners_and_reviewers_point_at_each_other() {
    let runner = by_id("website_runner").unwrap();
    assert_eq!(runner.file, "development/website/runner.md");
    assert_eq!(runner.summary, "Implements a website ticket and opens a pull request.");
    assert_eq!(runner.peers[0].id, "website_reviewer");
    assert_eq!(runner.repos[0].kind, "website");
    assert!(runner.repos[0].description.contains("backend, mobile, or document"));
    let reviewer = by_id("website_reviewer").unwrap();
    assert_eq!(reviewer.summary, "Reviews pull requests from website_runner.");
    assert_eq!(reviewer.peers[0].id, "website_runner");
    assert!(by_id("nobody").is_none());
}

#[test]
fn the_planner_uses_atlassian_and_every_repository() {
    let planner = by_id("requirement_planner").unwrap();
    let kinds: Vec<&str> = planner.repos.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, ["document", "backend", "website", "mobile"]);
    let mcps: Vec<&str> = planner.mcps.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(mcps, ["atlassian_jira", "atlassian_confluence"]);
    assert!(planner.tools.is_empty());
    assert!(by_id("requirement_document").unwrap().mcps.is_empty());
}

#[test]
fn guide_names() {
    assert_eq!(guide_file("  "), DEFAULT_GUIDE);
    assert_eq!(guide_file(" CLAUDE.md "), "CLAUDE.md");
    assert!(valid_guide("") && valid_guide("notes.MD"));
    assert!(!valid_guide("a/b.md") && !valid_guide(".md") && !valid_guide("notes.txt"));
}

#[test]
fn missing_documents_are_reported() {
    let agent = Agent {
        id: "x".into(),
        file: "requirement/none.md".into(),
        ..Default::default()
    };
    assert_eq!(
        agent.body().unwrap_err().to_string(),
        "read embedded agent x: open requirement/none.md: file does not exist"
    );
}
