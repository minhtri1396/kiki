use std::collections::HashMap;
use std::fs;

use super::*;
use crate::classify::classify;
use crate::testutil::{join, p, sv, tmp};

fn t(kind: TargetKind, name: &str) -> Target {
    Target::named(kind, name)
}

fn cl(event: &str, raw: &str) -> Vec<Target> {
    classify(event, raw.as_bytes())
}

#[test]
fn decide_allows_selected_jira_and_blocks_confluence() {
    let root = tmp();
    let root = p(&root);
    let policy = Policy {
        agent_id: "reader".into(),
        workspace: "kiki".into(),
        mcps: sv(&["atlassian_jira"]),
        roots: vec![root.clone()],
        ..Default::default()
    };
    let jira = policy.decide(
        &root,
        "",
        cl(
            "beforeMCPExecution",
            r#"{"server":"mcp-atlassian-kiki","tool_name":"jira_search"}"#,
        ),
    );
    assert!(jira.allow, "jira = {jira:?}");
    let page = policy.decide(
        &root,
        "",
        cl(
            "beforeMCPExecution",
            r#"{"server":"mcp-atlassian-kiki","tool_name":"confluence_get_page"}"#,
        ),
    );
    assert!(
        !page.allow && page.message.contains("confluence_get_page"),
        "confluence = {page:?}"
    );
    let other = policy.decide(&root, "", [t(TargetKind::Mcp, "mcp-codegraph-kiki")]);
    assert!(!other.allow, "codegraph was allowed");
}

#[test]
fn decide_keeps_repository_work_inside_the_selected_folder() {
    let root = tmp();
    let outside = tmp();
    let (root, outside) = (p(&root), p(&outside));
    let policy = Policy {
        agent_id: "reader".into(),
        roots: vec![root.clone()],
        ..Default::default()
    };
    let inside = policy.decide(
        &root,
        "",
        cl(
            "beforeReadFile",
            &format!(r#"{{"file_path":"{}"}}"#, join(&root, "notes.md")),
        ),
    );
    assert!(inside.allow, "inside = {inside:?}");
    let out = policy.decide(
        &root,
        "",
        cl(
            "beforeShellExecution",
            &format!(r#"{{"command":"cat {}"}}"#, join(&outside, "secret")),
        ),
    );
    assert!(!out.allow && out.message.contains(&outside), "outside = {out:?}");
    let shell = policy.decide(
        &root,
        "",
        cl("preToolUse", r#"{"tool_name":"Shell","tool_input":{"command":"ls"}}"#),
    );
    assert!(shell.allow, "shell = {shell:?}");
}

#[test]
fn decide_blocks_github_cli_and_rtk_unless_they_are_set() {
    let root = tmp();
    let root = p(&root);
    let without = Policy {
        agent_id: "reader".into(),
        roots: vec![root.clone()],
        ..Default::default()
    };
    let gh = without.decide(&root, "", cl("beforeShellExecution", r#"{"command":"gh pr create"}"#));
    assert!(!gh.allow && gh.message.contains("GitHub CLI"), "gh = {gh:?}");
    let rtk = without.decide(
        &root,
        "",
        cl(
            "preToolUse",
            r#"{"tool_name":"Shell","tool_input":{"command":"rtk git status"}}"#,
        ),
    );
    assert!(!rtk.allow && rtk.message.contains("RTK is off"), "rtk = {rtk:?}");
    let with = Policy {
        agent_id: "reader".into(),
        tools: sv(&["github_cli"]),
        roots: vec![root.clone()],
        rtk: true,
        ..Default::default()
    };
    let out = with.decide(
        &root,
        "",
        cl("beforeShellExecution", r#"{"command":"sudo -n gh pr list"}"#),
    );
    assert!(out.allow, "allowed gh = {out:?}");
    let out = with.decide(&root, "", cl("beforeShellExecution", r#"{"command":"rtk gh pr list"}"#));
    assert!(out.allow, "allowed rtk gh = {out:?}");
    let out = with.decide(&root, "", cl("beforeShellExecution", r#"{"command":"ls"}"#));
    assert!(out.allow, "ls = {out:?}");
    let other = tmp();
    let other = p(&other);
    let out = with.decide(
        &other,
        "",
        cl("beforeShellExecution", r#"{"command":"rtk git status"}"#),
    );
    assert!(!out.allow, "rtk outside = {out:?}");
}

#[test]
fn decide_blocks_tools_and_agents_that_were_not_selected() {
    let root = tmp();
    let root = p(&root);
    let policy = Policy {
        agent_id: "reader".into(),
        tools: sv(&["preview"]),
        agents: sv(&["planner"]),
        roots: vec![root.clone()],
        ..Default::default()
    };
    let tool = policy.decide(&root, "", [t(TargetKind::Tool, "web_search")]);
    assert!(!tool.allow && tool.message.contains("web_search"), "tool = {tool:?}");
    let ok_tool = policy.decide(&root, "", [t(TargetKind::Tool, "preview")]);
    assert!(ok_tool.allow, "preview = {ok_tool:?}");
    let peer = policy.decide(&root, "", cl("subagentStart", r#"{"subagent_type":"reviewer"}"#));
    assert!(!peer.allow && peer.message.contains("reviewer"), "peer = {peer:?}");
    let allowed = policy.decide(
        &root,
        "",
        cl(
            "preToolUse",
            r#"{"tool_name":"Task","tool_input":{"subagent_type":"planner"}}"#,
        ),
    );
    assert!(allowed.allow, "planner = {allowed:?}");
    let launch = policy.decide(
        &root,
        "",
        cl(
            "beforeShellExecution",
            r#"{"command":"orca agent launch --agent reviewer"}"#,
        ),
    );
    assert!(!launch.allow, "launch was allowed");
}

#[test]
fn prompt_has_no_interaction_to_block() {
    let raw = r#"{"hook_event_name":"UserPromptSubmit","prompt":"use the other repository"}"#;
    let got = cl("UserPromptSubmit", raw);
    assert!(got.is_empty(), "targets = {got:?}");
    let policy = Policy {
        agent_id: "reader".into(),
        ..Default::default()
    };
    let result = policy.decide("/tmp", "", Vec::<Target>::new());
    assert!(result.allow, "prompt = {result:?}");
}

#[test]
fn resolve_uses_the_started_agent_when_several_share_a_repository() {
    let root = tmp();
    let root = p(&root);
    let a = Policy {
        agent_id: "reader".into(),
        roots: vec![root.clone()],
        mcps: sv(&["atlassian_jira"]),
        ..Default::default()
    };
    let b = Policy {
        agent_id: "writer".into(),
        roots: vec![root.clone()],
        ..Default::default()
    };
    let mut q = Query {
        cwd: root.clone(),
        policies: vec![a, b],
        bindings: vec![Binding {
            agent_id: "reader".into(),
            worktree: root.clone(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let got = resolve(&q);
    assert!(
        got.status == Status::Use && got.policy.agent_id == "reader",
        "resolve = {got:?}"
    );
    q.bindings = Vec::new();
    let got = resolve(&q);
    assert!(
        got.status == Status::Deny && got.message.contains("reader") && got.message.contains("writer"),
        "ambiguous = {got:?}"
    );
    q.agent = "writer".into();
    let got = resolve(&q);
    assert!(
        got.status == Status::Use && got.policy.agent_id == "writer",
        "named = {got:?}"
    );
}

#[test]
fn resolve_allows_a_folder_that_is_not_a_repository() {
    let repo = tmp();
    let other = tmp();
    let (repo, other) = (p(&repo), p(&other));
    let got = resolve(Query {
        cwd: other,
        policies: vec![Policy {
            agent_id: "reader".into(),
            roots: vec![repo.clone()],
            ..Default::default()
        }],
        workspace: vec![repo.clone()],
        ..Default::default()
    });
    assert!(got.status == Status::Outside, "outside = {got:?}");
    let got = resolve(Query {
        cwd: repo.clone(),
        workspace: vec![repo],
        ..Default::default()
    });
    assert!(
        got.status == Status::Deny && got.message.contains("no agent"),
        "empty = {got:?}"
    );
}

#[test]
fn readonly_repository_allows_reads_and_blocks_writes() {
    let docs = tmp();
    let code = tmp();
    let (docs, code) = (p(&docs), p(&code));
    let policy = Policy {
        agent_id: "planner".into(),
        roots: vec![docs.clone(), code.clone()],
        read_only: vec![docs.clone()],
        ..Default::default()
    };
    let note = join(&docs, "notes.md");
    let read = policy.decide(&docs, "", cl("beforeReadFile", &format!(r#"{{"file_path":"{note}"}}"#)));
    assert!(read.allow, "read = {read:?}");
    let status = policy.decide(&docs, "", cl("beforeShellExecution", r#"{"command":"git status"}"#));
    assert!(status.allow, "status = {status:?}");
    let listed = policy.decide(
        &docs,
        "",
        cl("preToolUse", r#"{"tool_name":"Shell","tool_input":{"command":"ls"}}"#),
    );
    assert!(listed.allow, "ls = {listed:?}");
    let edit = policy.decide(
        &docs,
        "",
        cl(
            "preToolUse",
            &format!(r#"{{"tool_name":"Write","tool_input":{{"path":"{note}"}}}}"#),
        ),
    );
    assert!(!edit.allow && edit.message.contains("readonly"), "edit = {edit:?}");
    let commit = policy.decide(
        &docs,
        "",
        cl("beforeShellExecution", r#"{"command":"git commit -am msg"}"#),
    );
    assert!(
        !commit.allow && commit.message.contains("readonly"),
        "commit = {commit:?}"
    );
    let redirect = policy.decide(
        &docs,
        "",
        cl("beforeShellExecution", r#"{"command":"echo hi > notes.md"}"#),
    );
    assert!(!redirect.allow, "redirect = {redirect:?}");
    let inplace = policy.decide(
        &docs,
        "",
        cl("beforeShellExecution", r#"{"command":"sed -i '' 's/a/b/' notes.md"}"#),
    );
    assert!(!inplace.allow, "sed = {inplace:?}");
    let code_file = join(&code, "main.go");
    let change = policy.decide(
        &code,
        "",
        cl(
            "preToolUse",
            &format!(r#"{{"tool_name":"Edit","tool_input":{{"path":"{code_file}"}}}}"#),
        ),
    );
    assert!(change.allow, "writable repo = {change:?}");
    let cross = policy.decide(
        &code,
        "",
        cl(
            "preToolUse",
            &format!(r#"{{"tool_name":"Edit","tool_input":{{"path":"{note}"}}}}"#),
        ),
    );
    assert!(!cross.allow && cross.message.contains(&note), "cross edit = {cross:?}");
}

#[test]
fn readonly_worktree_blocks_writes() {
    let repo = tmp();
    let work = tmp();
    let (repo, work) = (p(&repo), p(&work));
    let got = resolve(Query {
        cwd: work.clone(),
        agent: "reader".into(),
        policies: vec![Policy {
            agent_id: "reader".into(),
            roots: vec![repo.clone()],
            read_only: vec![repo.clone()],
            ..Default::default()
        }],
        bindings: vec![Binding {
            agent_id: "reader".into(),
            worktree: work.clone(),
            read_only: true,
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(got.status == Status::Use, "resolve = {got:?}");
    let file = join(&work, "main.go");
    let edit = got.policy.decide(
        &work,
        "",
        cl(
            "preToolUse",
            &format!(r#"{{"tool_name":"Edit","tool_input":{{"path":"{file}"}}}}"#),
        ),
    );
    assert!(
        !edit.allow && edit.message.contains("readonly"),
        "worktree edit = {edit:?}"
    );
    let read = got
        .policy
        .decide(&work, "", cl("beforeReadFile", &format!(r#"{{"file_path":"{file}"}}"#)));
    assert!(read.allow, "worktree read = {read:?}");
}

#[test]
fn decide_blocks_an_nx_project_the_repository_did_not_select() {
    let root = tmp();
    let root = p(&root);
    write_project(&root, "apps/admin", "admin");
    write_project(&root, "apps/user", "user");
    fs::write(join(&root, "README.md"), "hi").unwrap();
    let policy = Policy {
        agent_id: "runner".into(),
        roots: vec![root.clone()],
        services: HashMap::from([(root.clone(), sv(&["admin"]))]),
        ..Default::default()
    };
    let admin = join(&root, "apps/admin/src/main.ts");
    let user = join(&root, "apps/user/src/main.ts");
    let readme = join(&root, "README.md");
    let read = |path: &str| cl("beforeReadFile", &format!(r#"{{"file_path":"{path}"}}"#));
    let got = policy.decide(&root, "", read(&admin));
    assert!(got.allow, "admin = {got:?}");
    let blocked = policy.decide(&root, "", read(&user));
    assert!(
        !blocked.allow && blocked.message.contains("service user") && blocked.message.contains("admin"),
        "user = {blocked:?}"
    );
    let got = policy.decide(&root, "", read(&readme));
    assert!(got.allow, "readme = {got:?}");
    let from_user = policy.decide(
        &join(&root, "apps/user"),
        "",
        cl("beforeShellExecution", r#"{"command":"ls"}"#),
    );
    assert!(
        !from_user.allow && from_user.message.contains("service user"),
        "cwd = {from_user:?}"
    );
    let open = Policy {
        agent_id: "runner".into(),
        roots: vec![root.clone()],
        ..Default::default()
    };
    let got = open.decide(&root, "", read(&user));
    assert!(got.allow, "unset services = {got:?}");
}

#[test]
fn worktree_keeps_the_services_selected_for_that_repository() {
    let repo = tmp();
    let work = tmp();
    let (repo, work) = (p(&repo), p(&work));
    write_project(&work, "apps/admin", "admin");
    write_project(&work, "apps/user", "user");
    let got = resolve(Query {
        cwd: work.clone(),
        agent: "runner".into(),
        policies: vec![Policy {
            agent_id: "runner".into(),
            roots: vec![repo.clone()],
            services: HashMap::from([(repo.clone(), sv(&["admin"]))]),
            ..Default::default()
        }],
        bindings: vec![Binding {
            agent_id: "runner".into(),
            worktree: work.clone(),
            services: sv(&["admin"]),
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(got.status == Status::Use, "resolve = {got:?}");
    let edit_json = |path: &str| {
        cl(
            "preToolUse",
            &format!(r#"{{"tool_name":"Edit","tool_input":{{"path":"{path}"}}}}"#),
        )
    };
    let admin = join(&work, "apps/admin/main.ts");
    let user = join(&work, "apps/user/main.ts");
    let edit = got.policy.decide(&work, "", edit_json(&admin));
    assert!(edit.allow, "admin = {edit:?}");
    let blocked = got.policy.decide(&work, "", edit_json(&user));
    assert!(
        !blocked.allow && blocked.message.contains("service user"),
        "user = {blocked:?}"
    );
}

fn write_project(root: &str, rel: &str, name: &str) {
    let dir = join(root, rel);
    fs::create_dir_all(&dir).unwrap();
    fs::write(join(&dir, "project.json"), format!(r#"{{"name":"{name}"}}"#)).unwrap();
}
