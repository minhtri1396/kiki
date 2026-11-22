use kiki_agentdef as agentdef;

use super::*;
use crate::snapshot::Repo;

fn repo_of(kind: &str) -> agentdef::Repo {
    agentdef::Repo {
        kind: kind.into(),
        ..Default::default()
    }
}

fn crepo(kind: &str, folder: &str) -> Repo {
    Repo {
        kind: kind.into(),
        folder: folder.into(),
        ..Default::default()
    }
}

fn must_contain(text: &str, wants: &[&str]) {
    for want in wants {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
}

#[test]
fn prompt_names_collaborators_by_id() {
    let agent = agentdef::Agent {
        id: "requirement_document".into(),
        title: "Document Writer".into(),
        repos: vec![agentdef::Repo {
            kind: "document".into(),
            description: "Write the requirement files in this folder.".into(),
            ..Default::default()
        }],
        mcps: vec![
            agentdef::Use {
                id: "atlassian_jira".into(),
                name: "Atlassian - Jira".into(),
                description: "Read the Jira ticket.\nWrite the comment.".into(),
                board_id: "12".into(),
                board_name: "KIKI".into(),
                ..Default::default()
            },
            agentdef::Use {
                id: "atlassian_confluence".into(),
                name: "Atlassian - Confluence".into(),
                description: "Read the design page.".into(),
                space_id: "111".into(),
                space_key: "ENG".into(),
                space_name: "Engineering".into(),
                parent_id: "99".into(),
                parent_title: "Requirements".into(),
                ..Default::default()
            },
        ],
        tools: vec![agentdef::Use {
            id: "github".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        setup_file: "/app/kiki.yaml".into(),
        workspace_id: "kiki".into(),
        repos: vec![Repo {
            kind: "document".into(),
            folder: "/app/docs".into(),
            index: "obsidian".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let peers = vec![Peer {
        id: "requirement_designer".into(),
        handoffs: vec![
            Handoff {
                when: "The requirement files are ready.".into(),
                send: "Hand over the requirement folder.\nWait for design.html.".into(),
                reply: "The design path.\nOpen questions.".into(),
                max_loops: 3,
            },
            Handoff {
                send: "Say the folder is blocked.".into(),
                ..Default::default()
            },
            Handoff {
                when: "No content".into(),
                send: " ".into(),
                ..Default::default()
            },
        ],
    }];
    let text = prompt(&agent, "Body\n", &snap, "", &peers);
    must_contain(
        &text,
        &[
            "KIKI started you with `orca agent launch`.",
            "Orca worktree: ",
            "You were started in the Orca worktree.",
            "Your agent id is `requirement_document`.",
            "- document",
            "Work in this repository:",
            "Write the requirement files in this folder.",
            "### requirement_designer",
            "When a condition is met, send that content to the agent through Orca",
            "#### Message 1 to requirement_designer",
            "**Send when**\n\nThe requirement files are ready.",
            "Hand over the requirement folder.",
            "**Report back: yes**",
            "- In this message, ask requirement_designer to report back to you, agent id requirement_document, when it has finished the work.",
            "- Use Orca for that report.",
            "**The report from requirement_designer must contain:**\n\nThe design path.\n\nOpen questions.",
            "This exchange can go back and forth between you and requirement_designer.",
            "Allow at most 3 loops.",
            "when the Send when condition of message 1 is met again after the report from requirement_designer",
            "\"loop 1 of 3\"",
            "After loop 3, do not send this message again",
            "#### Message 2 to requirement_designer",
            "**Send when:** (no condition set)",
            "Say the folder is blocked.",
            "**Report back: no.** Do not ask requirement_designer to report back.",
            "Wait for design.html.",
            "MCP servers for this agent:",
            "- Atlassian - Jira",
            "Board: KIKI (12)",
            "Use this MCP server:",
            "Space: Engineering (ENG)",
            "Parent page: Requirements (99)",
            "Read the Jira ticket.",
            "Write the comment.",
            "- Atlassian - Confluence",
            "Read the design page.",
            "Tools for this agent:",
            "- github",
            "### document — Obsidian",
            "Index file: /app/.kiki/repos/document.yaml",
            "obsidian vault=<name> search query=\"<words>\" limit=20",
            "No guide files are set for this repository. Do not invent one.",
        ],
    );
    assert!(
        text.matches("**Loop**").count() == 1 && text.matches("must contain:").count() == 1,
        "a message without a loop or a reply got one:\n{text}"
    );
    assert!(
        !text.contains("Message 3 to") && !text.contains("No content"),
        "a message with nothing to send was written:\n{text}"
    );
    assert!(
        !text.contains("codegraph explore") && !text.contains("Obsidian vault:"),
        "document agent prompt includes an index it is not set to use:\n{text}"
    );
}

#[test]
fn prompt_adds_codegraph_only_for_code_repos() {
    let agent = agentdef::Agent {
        id: "backend_runner".into(),
        title: "Backend Runner".into(),
        repos: vec![repo_of("backend")],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![Repo {
            kind: "backend".into(),
            folder: "/app/backend".into(),
            services: vec!["api".into()],
            index: "codegraph".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    must_contain(
        &text,
        &[
            "### backend — CodeGraph",
            "Folder: /app/backend",
            "Index file: /app/.kiki/repos/backend.yaml",
            "Services: api",
            "No guide files are set for this repository. Do not invent one.",
            "For service api, use this repository's guide files.",
            "codegraph explore -p /app/backend",
        ],
    );
    assert!(
        !text.contains("obsidian vault="),
        "backend agent prompt includes Obsidian:\n{text}"
    );
}

#[test]
fn prompt_names_the_sparse_checkout() {
    let agent = agentdef::Agent {
        id: "requirement_document".into(),
        title: "Document Writer".into(),
        repos: vec![repo_of("document")],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![Repo {
            kind: "document".into(),
            folder: "/app/docs".into(),
            sparse: "product/kiki".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    must_contain(
        &text,
        &[
            "checkout: product/kiki",
            "sparse checkout of product/kiki",
            "Read and change files only inside product/kiki",
            "git commit, git push, and git pull",
        ],
    );
}

#[test]
fn prompt_leaves_rtk_to_the_hook() {
    let agent = agentdef::Agent {
        id: "backend_runner".into(),
        title: "Backend Runner".into(),
        repos: vec![repo_of("backend")],
        tools: vec![agentdef::Use {
            id: "github_cli".into(),
            name: "GitHub CLI".into(),
            description: "The gh command opens pull requests, issues, and checks. Use gh for that work.".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![crepo("backend", "/app/backend")],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    assert!(
        !text.contains("RTK is on") && !text.contains("Use this tool:") && text.contains("- GitHub CLI"),
        "prompt =\n{text}"
    );
}

#[test]
fn prompt_omits_index_until_one_is_saved() {
    let agent = agentdef::Agent {
        id: "backend_runner".into(),
        title: "Backend Runner".into(),
        repos: vec![repo_of("backend"), repo_of("document")],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![
            crepo("backend", "/app/backend"),
            Repo {
                kind: "document".into(),
                folder: "/app/docs".into(),
                index: "obsidian".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    assert!(
        text.contains("### document — Obsidian"),
        "missing the saved Obsidian index:\n{text}"
    );
    assert!(
        !text.contains("### backend — CodeGraph") && !text.contains("codegraph explore"),
        "backend has no saved index and should be omitted:\n{text}"
    );
}

#[test]
fn prompt_names_every_repository_folder() {
    let agent = agentdef::Agent {
        id: "requirement_planner".into(),
        title: "Planner".into(),
        repos: vec![repo_of("document"), repo_of("backend"), repo_of("website")],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        worktree: "/app/docs".into(),
        repos: vec![
            crepo("document", "/app/docs"),
            crepo("backend", "/app/backend"),
            crepo("website", ""),
        ],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    must_contain(
        &text,
        &[
            "Orca worktrees:\n- document: /app/docs\n- backend: /app/backend\n- website: (not set)",
            "Each repository has its own Orca worktree, listed above.",
            "Do not share one worktree across repositories.",
            "- document\n  folder: /app/docs",
            "- backend\n  folder: /app/backend",
        ],
    );
    assert!(
        !text.contains("Orca worktree: /app/docs") && !text.contains("That folder is the repository for this launch."),
        "a launch with several repositories still names one folder as the worktree:\n{text}"
    );
}

#[test]
fn prompt_says_when_a_repository_is_readonly() {
    let agent = agentdef::Agent {
        id: "requirement_planner".into(),
        title: "Planner".into(),
        repos: vec![
            agentdef::Repo {
                kind: "document".into(),
                readonly: true,
                ..Default::default()
            },
            agentdef::Repo {
                kind: "backend".into(),
                description: "Change the API.".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![crepo("document", "/app/docs"), crepo("backend", "/app/backend")],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    let note = "Readonly: this agent can only read this repository.";
    assert!(text.contains(note), "missing readonly note:\n{text}");
    assert_eq!(
        text.matches(note).count(),
        1,
        "readonly note should be only on document:\n{text}"
    );
}

#[test]
fn prompt_names_the_guide_files_of_each_repository() {
    let agent = agentdef::Agent {
        id: "backend_runner".into(),
        title: "Backend Runner".into(),
        repos: vec![repo_of("backend"), repo_of("document")],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![
            Repo {
                kind: "backend".into(),
                folder: "/app/backend".into(),
                services: vec!["api".into(), "web".into()],
                divided: true,
                guides: vec!["RULES.md".into(), "DESIGN.md".into()],
                service_guides: vec![
                    crate::ServiceGuides {
                        service: "api".into(),
                        shared: true,
                        ..Default::default()
                    },
                    crate::ServiceGuides {
                        service: "web".into(),
                        guides: vec!["WEB.md".into()],
                        ..Default::default()
                    },
                ],
                index: "codegraph".into(),
                ..Default::default()
            },
            Repo {
                kind: "document".into(),
                folder: "/app/docs".into(),
                guides: vec!["AGENTS.md".into()],
                index: "obsidian".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    must_contain(
        &text,
        &[
            "guide files:\n    - /app/backend/RULES.md\n    - /app/backend/DESIGN.md",
            "Read these guide files and follow them whenever you work in this repository.",
            "service api: use this repository's guide files.",
            "service web guide files:\n    - /app/backend/WEB.md",
            "follow the service files.",
            "Before you read code, read /app/backend/RULES.md and /app/backend/DESIGN.md.",
            "For service web, read /app/backend/WEB.md.",
            "Read /app/docs/AGENTS.md before you search notes",
        ],
    );
}

#[test]
fn prompt_waiting_tells_a_supporting_agent_to_wait() {
    let agent = agentdef::Agent {
        id: "requirement_designer".into(),
        title: "Designer".into(),
        repos: vec![repo_of("document")],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![crepo("document", "/app/docs")],
        ..Default::default()
    };
    let req = |id: &str, title: &str| Requester {
        id: id.into(),
        title: title.into(),
    };
    let text = prompt_waiting(
        &agent,
        "Body\n",
        &snap,
        "Add a cart.",
        &[],
        &[
            req("requirement_document", "Document Writer"),
            req("requirement_designer", ""),
            req(" ", ""),
        ],
    );
    must_contain(
        &text,
        &[
            "WAIT FOR A REQUEST.",
            "listed under Agents Coordination by requirement_document (Document Writer).",
            "wait for a request from requirement_document (Document Writer), sent through Orca to your agent id",
            "It is context for the request that will reach you. It is not an instruction to start now.",
            "===== USER MESSAGE =====\nAdd a cart.",
        ],
    );
    assert!(
        !text.contains("Treat that message as the user message in your terminal")
            && !text.contains("from requirement_designer"),
        "waiting prompt still treats the message as an instruction or waits for itself:\n{text}"
    );
    let plain = prompt_waiting(
        &agent,
        "Body\n",
        &snap,
        "Add a cart.",
        &[],
        &[req("requirement_designer", "")],
    );
    assert!(
        !plain.contains("WAIT FOR A REQUEST.")
            && plain.contains("Treat that message as the user message in your terminal"),
        "an agent that only lists itself was told to wait:\n{plain}"
    );
}

#[test]
fn builder_sections_append_in_the_order_they_are_called() {
    let agent = agentdef::Agent {
        id: "planner".into(),
        title: "Planner".into(),
        file: "planner.md".into(),
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        ..Default::default()
    };
    let identity = PromptBuilder::new(&agent, &snap).identity().build();
    assert!(
        identity.starts_with("You are planner (Planner).\n\nKIKI started you with `orca agent launch`."),
        "{identity}"
    );
    assert!(identity.contains("Folder root: /app\nOrca worktree: \n"), "{identity}");
    let document = PromptBuilder::new(&agent, &snap)
        .agent_document("Body")
        .user_message(" Hi ")
        .build();
    assert_eq!(
        document,
        "===== AGENT DOCUMENT (planner.md) =====\nBody\n===== END AGENT DOCUMENT =====\n\n===== USER MESSAGE =====\nHi\n===== END USER MESSAGE =====\n"
    );
    let nothing = PromptBuilder::new(&agent, &snap)
        .repositories()
        .indexes()
        .mcp_servers()
        .tools()
        .build();
    assert_eq!(nothing, "Repositories for this agent:\n\n");
}

#[test]
fn builder_startup_notice_covers_every_case() {
    let agent = agentdef::Agent {
        id: "planner".into(),
        ..Default::default()
    };
    let notice = |requesters: &[Requester], note: &str| {
        PromptBuilder::new(&agent, &Snapshot::default())
            .startup_notice(requesters, note)
            .build()
    };
    assert!(notice(&[], "").starts_with("The user did not send a ticket"));
    assert!(notice(&[], "x").starts_with("The user sent a message with this launch."));
    let asker = Requester {
        id: "designer".into(),
        title: String::new(),
    };
    assert!(notice(std::slice::from_ref(&asker), "").starts_with("WAIT FOR A REQUEST."));
    assert!(!notice(std::slice::from_ref(&asker), "").contains("The user message at the end"));
    assert!(notice(&[asker], "x").contains("The user message at the end of this prompt is the task."));
}

#[test]
fn prompt_asks_for_required_input_the_user_left_out() {
    let agent = agentdef::Agent {
        id: "requirement_document".into(),
        title: "Document Writer".into(),
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        ..Default::default()
    };
    let body = "\
# Agent: Document Writer

## Input

- **Ticket**
  The Jira key.
  Required:

- **Scope**
  What the task covers.
  Required:

- **Status**
  Starting status.
  Default: draft
";
    let text = prompt(&agent, body, &snap, "Add a cart.", &[]);
    must_contain(
        &text,
        &[
            "Required input. These inputs are required: Ticket, Scope.",
            "ask the user for it before you continue. Do not guess.",
        ],
    );
    assert!(
        !text.contains("Status."),
        "optional input was treated as required:\n{text}"
    );
    let plain = prompt(&agent, "Body\n", &snap, "Add a cart.", &[]);
    assert!(
        !plain.contains("Required input."),
        "plain prompt added a required section:\n{plain}"
    );
    must_contain(
        &plain,
        &[
            "Questions for the user.",
            "A: text",
            "A: choice",
            "S: PROJ-12",
            "S: Website only",
            "===== QUESTIONS =====",
            "===== END QUESTIONS =====",
            "Name: value",
        ],
    );
}

#[test]
fn the_start_prompt_writes_the_app_root_into_the_agent_document() {
    let agent = agentdef::Agent {
        id: "requirement_document".into(),
        file: "requirement_document.md".into(),
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/Users/me/kiki".into(),
        setup_file: "/Users/me/kiki/.kiki/kiki.yaml".into(),
        workspace_id: "studio".into(),
        ..Default::default()
    };
    let body = "\
- **Root folder:** the `Folder root:` line in the launch prompt. Resolve `.kiki/agents` from that folder. Your working directory is the Orca worktree, the repository folder chosen for this launch. Do not assume a fixed absolute path.
- **Setup file:** its absolute path is the `Setup file:` line in the launch prompt. That file is `.kiki/kiki.yaml` next to the app. The app folder is the repository root.
- **Workspace:** `workspace_id` in that file is `<ws>`. There is one id. Do not assume it is `kiki`.
";
    let text = PromptBuilder::new(&agent, &snap).agent_document(body).build();
    must_contain(
        &text,
        &[
            "- **Root folder:** /Users/me/kiki. Resolve `.kiki/agents` from that folder.",
            "- **Setup file:** /Users/me/kiki/.kiki/kiki.yaml. That file is `.kiki/kiki.yaml` next to the app. The app folder is the repository root.",
            "`workspace_id` in that file is `studio`. There is one id. Do not assume it is `kiki`.",
        ],
    );
    assert!(
        !text.contains("Do not assume a fixed absolute path") && !text.contains("<ws>"),
        "text = {text}"
    );
}

#[test]
fn prompt_lists_services_only_when_the_repository_is_divided() {
    let agent = agentdef::Agent {
        id: "backend_runner".into(),
        title: "Backend Runner".into(),
        repos: vec![repo_of("backend"), repo_of("website")],
        ..Default::default()
    };
    let snap = Snapshot {
        root: "/app".into(),
        repos: vec![
            Repo {
                kind: "backend".into(),
                folder: "/app/backend".into(),
                divided: true,
                ..Default::default()
            },
            crepo("website", "/app/website"),
        ],
        ..Default::default()
    };
    let text = prompt(&agent, "Body\n", &snap, "", &[]);
    must_contain(
        &text,
        &[
            "services: (none)",
            "This repository is divided into Nx services. None are selected, so do not use a service.",
            "A repository with no services line is not divided into services.",
        ],
    );
    assert_eq!(text.matches("services:").count(), 1, "website listed services:\n{text}");
}
