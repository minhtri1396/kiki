//! The launch prompt must stay byte-for-byte what users' agents already receive: a rich setup is
//! rendered and compared with the stored text in `tests/golden/`.

use kiki_agentdef as agentdef;
use kiki_console::{prompt, prompt_waiting, Handoff, Peer, Repo, Requester, Snapshot};

fn agent() -> agentdef::Agent {
    let use_of = |id: &str, name: &str, description: &str| agentdef::Use {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        ..Default::default()
    };
    agentdef::Agent {
        id: "requirement_planner".into(),
        title: "Planner".into(),
        file: "requirement_planner.md".into(),
        repos: vec![
            agentdef::Repo {
                kind: "backend".into(),
                guide: "RULES.md".into(),
                description: "Change the API.\nKeep it small.".into(),
                ..Default::default()
            },
            agentdef::Repo {
                kind: "document".into(),
                readonly: true,
                ..Default::default()
            },
            agentdef::Repo {
                kind: "website".into(),
                description: "  ".into(),
                ..Default::default()
            },
            agentdef::Repo {
                kind: "mobile".into(),
                readonly: true,
                ..Default::default()
            },
            agentdef::Repo {
                kind: " ".into(),
                ..Default::default()
            },
            agentdef::Repo {
                kind: "extra".into(),
                ..Default::default()
            },
        ],
        mcps: vec![
            agentdef::Use {
                board_id: "12".into(),
                board_name: "KIKI".into(),
                space_id: "111".into(),
                space_name: "Engineering".into(),
                parent_id: "99".into(),
                parent_title: "Requirements".into(),
                ..use_of(
                    "atlassian_jira",
                    "Atlassian - Jira",
                    "Read the ticket.\nWrite the comment.",
                )
            },
            agentdef::Use {
                space_key: "ENG".into(),
                ..use_of("atlassian_confluence", "", "")
            },
            use_of(" ", "skipped", "skipped"),
        ],
        tools: vec![use_of("github", "GitHub CLI", "Open pull requests.")],
        ..Default::default()
    }
}

fn snapshot(orchestrator: &str) -> Snapshot {
    let repo = |kind: &str, folder: &str, index: &str, sparse: &str, services: &[&str]| Repo {
        kind: kind.into(),
        folder: folder.into(),
        index: index.into(),
        sparse: sparse.into(),
        services: services.iter().map(|s| s.to_string()).collect(),
    };
    Snapshot {
        root: "/app".into(),
        setup_file: "/app/.kiki/kiki.yaml".into(),
        worktree: "/work/tree".into(),
        workspace_id: "studio".into(),
        orchestrator: orchestrator.into(),
        repos: vec![
            repo("backend", "/app/backend/", "codegraph", "", &["api", "worker"]),
            repo("website", "/app/website", "", "", &[]),
            repo("mobile", "", "codegraph", "apps/mobile", &["app"]),
            repo("document", "/app/docs", "obsidian", "product/kiki", &[]),
            repo("extra", "/app/extra", "obsidian", "", &[]),
        ],
        ..Default::default()
    }
}

fn peers() -> Vec<Peer> {
    vec![
        Peer {
            id: "requirement_designer".into(),
            handoffs: vec![
                Handoff {
                    when: "The files are ready.\nAnd checked.".into(),
                    send: "Hand over the folder.".into(),
                    reply: "The design path.\nQuestions.".into(),
                    max_loops: 3,
                },
                Handoff {
                    send: "No condition.".into(),
                    ..Default::default()
                },
                Handoff {
                    send: "Reply without loops.".into(),
                    reply: "ok".into(),
                    max_loops: 0,
                    ..Default::default()
                },
                Handoff {
                    when: "skipped".into(),
                    send: " ".into(),
                    ..Default::default()
                },
            ],
        },
        Peer {
            id: "requirement_tester".into(),
            handoffs: vec![],
        },
        Peer {
            id: "requirement_planner".into(),
            handoffs: vec![],
        },
        Peer {
            id: " ".into(),
            handoffs: vec![],
        },
    ]
}

fn render() -> String {
    let mut out = String::new();
    let section = |out: &mut String, name: &str, text: String| {
        out.push_str(&format!("######## {name}\n{text}\n"));
    };
    let agent = agent();
    section(
        &mut out,
        "rich",
        prompt(
            &agent,
            "Body without newline",
            &snapshot("orca"),
            "Add a cart.\nQuickly.",
            &peers(),
        ),
    );
    section(
        &mut out,
        "no note",
        prompt(&agent, "Body\n", &snapshot("orca"), "  ", &[]),
    );
    section(
        &mut out,
        "other orchestrator",
        prompt(&agent, "Body\n", &snapshot("zeta"), "", &peers()),
    );
    let waiting = [
        Requester {
            id: "requirement_document".into(),
            title: "Document Writer".into(),
        },
        Requester {
            id: "requirement_designer".into(),
            title: String::new(),
        },
        Requester {
            id: "requirement_planner".into(),
            title: "self".into(),
        },
        Requester {
            id: " ".into(),
            title: String::new(),
        },
    ];
    section(
        &mut out,
        "waiting with note",
        prompt_waiting(&agent, "Body\n", &snapshot("orca"), "Add a cart.", &peers(), &waiting),
    );
    section(
        &mut out,
        "waiting no note",
        prompt_waiting(&agent, "Body\n", &snapshot("orca"), "", &[], &waiting),
    );
    let bare = agentdef::Agent {
        id: "bare".into(),
        title: "Bare".into(),
        file: "bare.md".into(),
        ..Default::default()
    };
    section(&mut out, "bare", prompt(&bare, "", &Snapshot::default(), "", &[]));
    out
}

#[test]
fn prompt_matches_the_stored_text() {
    let text = render();
    if let Ok(path) = std::env::var("KIKI_WRITE_GOLDEN") {
        std::fs::write(path, &text).unwrap();
        return;
    }
    let want = include_str!("golden/prompt.txt");
    assert!(
        text == want,
        "the launch prompt changed; run with KIKI_WRITE_GOLDEN=<path> to inspect it"
    );
}
