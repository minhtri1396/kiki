use super::*;

use std::fs;
use std::path::Path;

#[test]
fn save_rewrites_file_and_moves_category() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "backend_runner");
    let mut next = current.clone();
    next.id = "backend_builder".into();
    next.name = "Backend Builder".into();
    next.category = "platform".into();
    next.enabled = false;
    next.ai_tool = "codex".into();
    next.model = "gpt-5.6-sol".into();
    next.prompt = format!("{}Custom prompt line.\n", current.prompt);
    next.works_with = vec![
        Collaborator {
            id: "backend_reviewer".into(),
            ..Default::default()
        },
        Collaborator {
            id: "website_runner".into(),
            ..Default::default()
        },
    ];
    save(&dir, "backend_runner", &next).unwrap();
    assert!(
        !Path::new(&dir).join("backend").join("backend_runner.md").exists(),
        "old agent file still exists"
    );
    let text = fs::read_to_string(Path::new(&dir).join("platform").join("backend_builder.md")).unwrap();
    for want in [
        "id: backend_builder\n",
        "name: Backend Builder\n",
        "enabled: false\n",
        "ai_tool: codex\n",
        "model: gpt-5.6-sol\n",
        "- backend_reviewer\n",
        "- website_runner\n",
        "Custom prompt line.\n",
        "**Merge rights:**",
    ] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
    assert!(Path::new(&dir).join("backend").join("backend_reviewer.md").exists());
    let mut again = next.clone();
    again.name = "Taken".into();
    let err = save(&dir, "backend_reviewer", &again).unwrap_err();
    assert!(
        is_exists(&err) && err.downcast_ref::<ErrExists>().is_some(),
        "err = {err}"
    );
}

#[test]
fn save_keeps_handoffs() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "requirement_document");
    let mut next = current.clone();
    next.works_with = vec![Collaborator {
        id: "requirement_designer".into(),
        handoffs: vec![
            Handoff {
                when: "business_requirement.md and product_requirement.md are ready".into(),
                send: "Hand over the requirement folder.\nWait for design.html.".into(),
                reply: " The design path. ".into(),
                max_loops: 3,
            },
            Handoff {
                when: "  ".into(),
                send: "Answer the designer questions.".into(),
                max_loops: 5,
                ..Default::default()
            },
            Handoff {
                when: "".into(),
                send: "  ".into(),
                ..Default::default()
            },
        ],
    }];
    save(&dir, "requirement_document", &next).unwrap();
    let saved = find_ok(&dir, "requirement_document");
    assert!(
        saved.works_with.len() == 1 && saved.works_with[0].id == "requirement_designer",
        "works with = {:?}",
        saved.works_with
    );
    let want = vec![
        Handoff {
            when: "business_requirement.md and product_requirement.md are ready".into(),
            send: "Hand over the requirement folder.\nWait for design.html.".into(),
            reply: "The design path.".into(),
            max_loops: 3,
        },
        Handoff {
            send: "Answer the designer questions.".into(),
            ..Default::default()
        },
    ];
    assert_eq!(
        saved.works_with[0].handoffs, want,
        "handoffs = {:?}",
        saved.works_with[0].handoffs
    );
    let text = fs::read_to_string(&saved.path).unwrap();
    assert!(
        text.contains("agents_coordination:\n")
            && text.contains("id: requirement_designer\n")
            && text.contains("handoffs:\n")
            && text.contains("when: business_requirement.md")
            && text.contains("send:")
            && text.contains("max_loops: 3")
            && text.contains("reply: The design path.")
            && !text.contains("max_loops: 5")
            && !text.contains("description: Hand over")
            && !text.contains("\nteamwork:")
            && !text.contains("works_with:")
            && !text.contains("collaboration:"),
        "front matter = {text}"
    );
}

#[test]
fn save_keeps_mcp_and_tool_notes() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "requirement_document");
    assert!(
        current.mcps.is_empty() && current.tools.is_empty(),
        "seeded uses = {:?} {:?}",
        current.mcps,
        current.tools
    );
    let mut next = current.clone();
    next.mcps = vec![
        Use {
            id: "atlassian_jira".into(),
            description: "Read the Jira ticket.\nWrite the comment.".into(),
            board_id: "12".into(),
            board_name: "KIKI".into(),
            ..Default::default()
        },
        Use {
            id: "atlassian_confluence".into(),
            description: "Read the design page.".into(),
            space_id: "111".into(),
            space_key: "ENG".into(),
            space_name: "Engineering".into(),
            parent_id: "99".into(),
            parent_title: "Requirements".into(),
            ..Default::default()
        },
    ];
    next.tools = vec![Use {
        id: "github".into(),
        ..Default::default()
    }];
    save(&dir, "requirement_document", &next).unwrap();
    let saved = find_ok(&dir, "requirement_document");
    assert!(
        saved.mcps.len() == 2
            && saved.mcps[0].id == "atlassian_jira"
            && saved.mcps[0].description == "Read the Jira ticket.\nWrite the comment."
            && saved.mcps[0].board_id == "12"
            && saved.mcps[0].board_name == "KIKI"
            && saved.mcps[1].space_key == "ENG"
            && saved.mcps[1].parent_title == "Requirements",
        "mcps = {:?}",
        saved.mcps
    );
    assert!(
        saved.tools.len() == 1 && saved.tools[0].id == "github" && saved.tools[0].description.is_empty(),
        "tools = {:?}",
        saved.tools
    );
    let text = fs::read_to_string(&saved.path).unwrap();
    assert!(
        text.contains("mcps:\n")
            && text.contains("id: atlassian_jira\n")
            && text.contains("board_id: \"12\"\n")
            && text.contains("id: atlassian_confluence\n")
            && text.contains("space_key: ENG\n")
            && text.contains("parent_title: Requirements\n")
            && text.contains("tools:\n")
            && text.contains("- github\n"),
        "front matter = {text}"
    );
    next.mcps = Vec::new();
    next.tools = vec![Use {
        id: "GitHub".into(),
        ..Default::default()
    }];
    assert!(
        save(&dir, "requirement_document", &next).is_err(),
        "tool id with a capital letter was accepted"
    );
}

#[test]
fn save_keeps_readonly_repository() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "requirement_planner");
    let mut next = current.clone();
    next.repos = vec![
        Repository {
            kind: "document".into(),
            description: "Read the notes.".into(),
            readonly: true,
            ..Default::default()
        },
        Repository {
            kind: "backend".into(),
            readonly: true,
            ..Default::default()
        },
        Repository {
            kind: "website".into(),
            description: "Change the website.".into(),
            ..Default::default()
        },
    ];
    save(&dir, &current.id, &next).unwrap();
    let saved = find_ok(&dir, &current.id);
    assert!(
        saved.repos.len() == 3
            && saved.repos[0].readonly
            && saved.repos[0].description == "Read the notes."
            && saved.repos[1].readonly
            && saved.repos[1].description.is_empty()
            && !saved.repos[2].readonly,
        "repos = {:?}",
        saved.repos
    );
    let text = fs::read_to_string(&saved.path).unwrap();
    assert!(
        text.contains("readonly: true") && text.matches("readonly: true").count() == 2,
        "readonly was not stored on the two chosen repositories:\n{text}"
    );
}

#[test]
fn save_rejects_condition_without_message() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "requirement_document");
    let mut next = current.clone();
    next.works_with = vec![Collaborator {
        id: "requirement_designer".into(),
        handoffs: vec![Handoff {
            when: "the files are ready".into(),
            ..Default::default()
        }],
    }];
    assert!(
        save(&dir, "requirement_document", &next).is_err(),
        "a condition with nothing to send was saved"
    );
}

#[test]
fn save_keeps_repository_guide() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "requirement_document");
    let mut next = current.clone();
    next.repos = vec![Repository {
        kind: "document".into(),
        guide: " RULES.md ".into(),
        description: "Write the requirement.".into(),
        ..Default::default()
    }];
    save(&dir, "requirement_document", &next).unwrap();
    let saved = find_ok(&dir, "requirement_document");
    assert!(
        saved.repos.len() == 1 && saved.repos[0].guide == "RULES.md",
        "repos = {:?}",
        saved.repos
    );
    let raw = fs::read_to_string(&saved.path).unwrap();
    assert!(raw.contains("guide: RULES.md"), "front matter = {raw}");

    next.repos = vec![Repository {
        kind: "document".into(),
        guide: "AGENTS.md".into(),
        description: "Write the requirement.".into(),
        ..Default::default()
    }];
    save(&dir, "requirement_document", &next).unwrap();
    let saved = find_ok(&dir, "requirement_document");
    assert!(
        saved.repos[0].guide.is_empty(),
        "default guide was stored: {:?}",
        saved.repos
    );
    let raw = fs::read_to_string(&saved.path).unwrap();
    assert!(!raw.contains("guide:"), "default guide was written:\n{raw}");

    for bad in ["docs/AGENTS.md", "../AGENTS.md", "AGENTS.txt", ".md", "a\\b.md"] {
        next.repos = vec![Repository {
            kind: "document".into(),
            guide: bad.into(),
            ..Default::default()
        }];
        assert!(
            save(&dir, "requirement_document", &next).is_err(),
            "guide {bad:?} was accepted"
        );
    }
}

#[test]
fn save_rejects_loop_limit_outside_range() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "requirement_document");
    let mut next = current.clone();
    for limit in [-1, MAX_HANDOFF_LOOPS + 1] {
        next.works_with = vec![Collaborator {
            id: "requirement_designer".into(),
            handoffs: vec![Handoff {
                send: "Design it.".into(),
                reply: "The design path.".into(),
                max_loops: limit,
                ..Default::default()
            }],
        }];
        assert!(
            save(&dir, "requirement_document", &next).is_err(),
            "loop limit {limit} was saved"
        );
    }
    next.works_with = vec![Collaborator {
        id: "requirement_designer".into(),
        handoffs: vec![Handoff {
            send: "Design it.".into(),
            reply: "The design path.".into(),
            max_loops: MAX_HANDOFF_LOOPS,
            ..Default::default()
        }],
    }];
    save(&dir, "requirement_document", &next).unwrap();
}

#[test]
fn handoff_without_reply_does_not_report_back() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "requirement_document");
    let mut next = current.clone();
    next.works_with = vec![Collaborator {
        id: "requirement_designer".into(),
        handoffs: vec![Handoff {
            send: "Design it.".into(),
            reply: "   ".into(),
            max_loops: 4,
            ..Default::default()
        }],
    }];
    save(&dir, "requirement_document", &next).unwrap();
    let saved = find_ok(&dir, "requirement_document");
    let got = &saved.works_with[0].handoffs;
    assert!(
        got.len() == 1 && got[0].reply.is_empty() && got[0].max_loops == 0,
        "handoffs = {got:?}"
    );
}
