//! Tests for the setup files: MCP and tool settings, repository files, migration, team folder.

use std::fs;
use std::path::Path as FsPath;

use kiki_console as console;
use kiki_mcp::{Atlassian, Service};
use kiki_tools::Settings;

use super::*;

#[test]
fn atlassian_stays_out_of_setup_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = Store::open(&join(dir.path(), "kiki.yaml")).unwrap();
    let cfg = Atlassian {
        enabled: true,
        username: "a@acme.co".into(),
        api_token: "secret-jira-token".into(),
        jira: Service {
            enabled: true,
            url: "https://acme.atlassian.net".into(),
            ..Default::default()
        },
        confluence: Service {
            enabled: true,
            url: "https://acme.atlassian.net/wiki".into(),
            own: true,
            username: "wiki@acme.co".into(),
            api_token: "secret-wiki-token".into(),
            ..Default::default()
        },
    };
    db.save_atlassian(&cfg).unwrap();
    let got = db.atlassian().unwrap();
    assert!(
        got.api_token == "secret-jira-token"
            && got.confluence.api_token == "secret-wiki-token"
            && got.confluence.own
            && got.enabled,
        "loaded = {got:?}"
    );
    let info = fs::metadata(dir.path().join("mcp.yaml")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(info.permissions().mode() & 0o777, 0o600);
    assert!(
        !exists(dir.path().join("atlassian.yaml")),
        "credentials were written to atlassian.yaml"
    );
    let setup = fs::read_to_string(db.file()).unwrap();
    assert!(
        !setup.contains("secret-jira-token") && !setup.contains("atlassian"),
        "setup file contains Atlassian settings:\n{setup}"
    );
    let text = fs::read_to_string(dir.path().join("mcp.yaml")).unwrap();
    assert!(
        text.contains("secret-jira-token")
            && text.contains("Agents do not read this file")
            && text.contains("atlassian:"),
        "mcp file =\n{text}"
    );
    db.save_atlassian(&Atlassian::default()).unwrap();
    assert!(
        !exists(dir.path().join("mcp.yaml")),
        "empty setup left the credentials file"
    );
    db.close().unwrap();
}

#[test]
fn atlassian_file_moves_into_mcp_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = Store::open(&join(dir.path(), "kiki.yaml")).unwrap();
    let legacy = dir.path().join("atlassian.yaml");
    let body = "enabled: true\nusername: a@acme.co\napi_token: secret-jira-token\njira:\n  enabled: true\n  url: https://acme.atlassian.net\n";
    fs::write(&legacy, body).unwrap();
    let got = db.atlassian().unwrap();
    assert!(
        got.api_token == "secret-jira-token" && got.jira.enabled,
        "loaded = {got:?}"
    );
    assert!(!exists(&legacy), "legacy atlassian.yaml was kept");
    let raw = fs::read_to_string(dir.path().join("mcp.yaml")).unwrap();
    assert!(
        raw.contains("atlassian:") && raw.contains("secret-jira-token"),
        "mcp file =\n{raw}"
    );
}

#[test]
fn migrate_moves_app_files_and_leaves_repos() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let old_agents = root.join("agents");
    fs::create_dir_all(old_agents.join("requirement")).unwrap();
    fs::write(old_agents.join("requirement").join("requirement_document.md"), "agent").unwrap();
    fs::create_dir_all(root.join("backend")).unwrap();
    let setup = format!(
        "workspace_id: studio\norca_agent: cursor\nagent_path: {}\n",
        s(&old_agents)
    );
    fs::write(root.join("kiki.yaml"), setup).unwrap();
    fs::write(root.join("models.yaml"), "agents: []\n").unwrap();
    fs::write(root.join("kiki.db"), "db").unwrap();
    migrate(&s(root)).unwrap();
    assert!(!exists(root.join("kiki.yaml")), "setup file stayed beside the app");
    assert!(exists(root.join("backend")));
    let moved = root
        .join(".kiki")
        .join("agents")
        .join("requirement")
        .join("requirement_document.md");
    assert!(exists(moved));
    let text = fs::read_to_string(root.join(".kiki").join("kiki.yaml")).unwrap();
    assert!(
        !text.contains("agent_path:") && text.contains("workspace_id: studio"),
        "setup =\n{text}"
    );
    assert!(!exists(root.join("models.yaml")), "old models.yaml was kept");
    assert!(
        !exists(root.join(".kiki").join("models.yaml")),
        "models.yaml was copied into .kiki"
    );
    assert!(exists(root.join(".kiki").join("kiki.db")));
    migrate(&s(root)).unwrap();
}

#[test]
fn migrate_moves_custom_agents_into_kiki() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let custom = root.join("shared-agents").join("requirement");
    fs::create_dir_all(&custom).unwrap();
    fs::write(custom.join("requirement_document.md"), "shared").unwrap();
    fs::create_dir_all(root.join("agents")).unwrap();
    let setup = format!(
        "workspace_id: studio\norca_agent: cursor\nagent_path: {}\n",
        s(custom.parent().unwrap())
    );
    fs::write(root.join("kiki.yaml"), setup).unwrap();
    migrate(&s(root)).unwrap();
    assert!(exists(root.join("agents")));
    assert!(!exists(custom.parent().unwrap()), "custom agent folder was kept");
    let got = fs::read_to_string(
        root.join(".kiki")
            .join("agents")
            .join("requirement")
            .join("requirement_document.md"),
    )
    .unwrap();
    assert_eq!(got, "shared", "agent file = {got:?}");
    let raw = fs::read_to_string(path(&s(root))).unwrap();
    assert!(!raw.contains("agent_path:"), "setup =\n{raw}");
}

fn open_root() -> (tempfile::TempDir, String, Store) {
    let dir = tempfile::tempdir().unwrap();
    let root = s(dir.path());
    let db = Store::open(&path(&root)).unwrap();
    (dir, root, db)
}

fn set_services(snap: &mut console::Snapshot, kind: &str, services: &[&str]) {
    for repo in snap.repos.iter_mut() {
        if repo.kind == kind {
            repo.services = services.iter().map(|s| s.to_string()).collect();
        }
    }
}

#[test]
fn repo_files_keep_the_saved_index() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let repos = FsPath::new(&root).join(".kiki").join("repos");
    let backend = fs::read_to_string(repos.join("backend.yaml")).unwrap();
    assert!(
        backend.contains("id: backend") && !backend.contains("kind:") && !backend.contains("folder:"),
        "backend file:\n{backend}"
    );
    assert!(!backend.contains("index:"), "index was set before setup:\n{backend}");

    let snap = db.load().unwrap();
    db.save_repo_index(&snap, "document", "obsidian").unwrap();
    assert!(
        db.save_repo_index(&snap, "backend", "nope").is_err(),
        "expected an unknown index to be rejected"
    );
    let mut snap = db.load().unwrap();
    assert!(
        snap.repo("document").index == "obsidian" && snap.repo("backend").index.is_empty(),
        "indexes = document {:?} backend {:?}",
        snap.repo("document").index,
        snap.repo("backend").index
    );

    set_services(&mut snap, "backend", &["api"]);
    db.save(&snap).unwrap();
    db.ensure_layout(&root).unwrap();
    let snap = db.load().unwrap();
    assert_eq!(
        snap.repo("document").index,
        "obsidian",
        "document index = {:?}",
        snap.repo("document").index
    );
    assert_eq!(snap.repo("backend").services, vec!["api".to_string()]);
    let doc = fs::read_to_string(repos.join("document.yaml")).unwrap();
    assert!(doc.contains("index: obsidian"), "document file:\n{doc}");
    db.save_repo_index(&snap, "document", "").unwrap();
    let snap = db.load().unwrap();
    assert_eq!(
        snap.repo("document").index,
        "",
        "document index = {:?}",
        snap.repo("document").index
    );
    let doc = fs::read_to_string(repos.join("document.yaml")).unwrap();
    assert!(!doc.contains("\nindex:"), "index was kept:\n{doc}");
    assert_eq!(snap.repo("backend").services, vec!["api".to_string()]);
}

#[test]
fn remove_index_folder_deletes_only_that_directory() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let index = repo.join(".codegraph");
    fs::create_dir(&index).unwrap();
    fs::write(index.join("db"), "x").unwrap();
    let kept = repo.join("AGENTS.md");
    fs::write(&kept, "rules").unwrap();
    let other = repo.join(".obsidian");
    fs::create_dir(&other).unwrap();
    remove_index_folder(&s(repo), "codegraph").unwrap();
    assert!(!exists(&index), ".codegraph still exists");
    assert!(exists(&kept));
    assert!(exists(&other), ".obsidian was removed");
    remove_index_folder(&s(repo), "codegraph").unwrap();
    let link_target = tempfile::tempdir().unwrap();
    fs::write(link_target.path().join("keep"), "y").unwrap();
    std::os::unix::fs::symlink(link_target.path(), &index).unwrap();
    assert!(
        remove_index_folder(&s(repo), "codegraph").is_err(),
        "expected a link to be rejected"
    );
    assert!(exists(link_target.path().join("keep")), "link target was removed");
}

#[test]
fn repo_file_reads_legacy_kind() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let file = FsPath::new(&root).join(".kiki").join("repos").join("backend.yaml");
    let legacy = format!("kind: backend\nfolder: {root}/backend\nservices:\n    - api\n");
    fs::write(&file, legacy).unwrap();
    let snap = db.load().unwrap();
    assert_eq!(snap.repo("backend").services, vec!["api".to_string()]);
}

#[test]
fn repo_files_keep_the_sparse_checkout() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let snap = db.load().unwrap();
    db.save_repo_sparse(&snap, "document", "product/kiki").unwrap();
    assert!(
        db.save_repo_sparse(&snap, "document", "../secret").is_err(),
        "expected an invalid folder to be rejected"
    );
    let mut snap = db.load().unwrap();
    assert_eq!(snap.repo("document").sparse, "product/kiki");
    set_services(&mut snap, "backend", &["api"]);
    db.save(&snap).unwrap();
    let snap = db.load().unwrap();
    assert_eq!(
        snap.repo("document").sparse,
        "product/kiki",
        "sparse after save = {:?}",
        snap.repo("document").sparse
    );
    let doc = fs::read_to_string(FsPath::new(&root).join(".kiki").join("repos").join("document.yaml")).unwrap();
    assert!(doc.contains("sparse: product/kiki"), "document file:\n{doc}");
}

#[test]
fn save_repo_index_requires_a_known_repository() {
    let (_dir, _root, db) = open_root();
    assert!(
        db.save_repo_index(&console::Snapshot::default(), "docs", "obsidian")
            .is_err(),
        "expected an unknown repository"
    );
}

#[test]
fn save_repo_folder_keeps_the_path() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let custom_dir = tempfile::tempdir().unwrap();
    let custom = s(custom_dir.path());
    let snap = db.load().unwrap();
    db.save_repo_folder(&snap, "backend", &custom).unwrap();
    assert!(
        db.save_repo_folder(&snap, "backend", "relative/path").is_err(),
        "expected a relative path to be rejected"
    );
    assert!(
        db.save_repo_folder(&snap, "backend", &format!("{root}/missing-folder"))
            .is_err(),
        "expected a missing folder to be rejected"
    );
    db.ensure_layout(&root).unwrap();
    let snap = db.load().unwrap();
    assert_eq!(snap.repo("backend").folder, custom);
    assert_eq!(snap.repo("website").folder, "");
    assert!(!exists(FsPath::new(&root).join("website")));
    assert!(!exists(FsPath::new(&root).join("docs")));
    let saved = fs::read_to_string(FsPath::new(&root).join(".kiki").join("kiki.yaml")).unwrap();
    assert!(saved.contains(&custom), "kiki.yaml:\n{saved}");
}

#[test]
fn save_keeps_team_folder() {
    let dir = tempfile::tempdir().unwrap();
    let file = join(dir.path(), "kiki.yaml");
    let initial = "workspace_id: kiki\norca_agent: cursor\ngoogle_drive:\n  refresh_token: refresh-secret-token\n";
    fs::write(&file, initial).unwrap();
    let db = Store::open(&file).unwrap();
    db.save_team_folder(&TeamFolder {
        path: "  /tmp/kiki-team  ".into(),
        member: "  alice  ".into(),
        ..Default::default()
    })
    .unwrap();
    db.save(&console::Snapshot {
        workspace_id: "studio".into(),
        ..Default::default()
    })
    .unwrap();
    let cfg = db.team_folder().unwrap();
    assert!(cfg.path == "/tmp/kiki-team" && cfg.member == "alice", "team = {cfg:?}");
    let snap = db.load().unwrap();
    assert_eq!(snap.workspace_id, "studio");
    let text = fs::read_to_string(db.file()).unwrap();
    assert!(
        text.contains("path: /tmp/kiki-team")
            && text.contains("member: alice")
            && text.contains("workspace_id: studio"),
        "setup file =\n{text}"
    );
    assert!(
        !text.contains("refresh-secret-token") && !text.contains("google_drive"),
        "google drive settings were kept:\n{text}"
    );
    assert!(!text.contains("orca_agent"), "orca_agent was kept:\n{text}");
}

#[test]
fn open_drops_orca_agent() {
    let dir = tempfile::tempdir().unwrap();
    let file = join(dir.path(), "kiki.yaml");
    let initial =
        "workspace_id: studio\norchestrator: orca\norca_agent: claude\ngenerate_tool: cursor\ngenerate_model: auto\n";
    fs::write(&file, initial).unwrap();
    let db = Store::open(&file).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(!text.contains("orca_agent"), "setup =\n{text}");
    let snap = db.load().unwrap();
    assert!(
        snap.workspace_id == "studio"
            && snap.orchestrator == "orca"
            && snap.generate_tool == "cursor"
            && snap.generate_model == "auto",
        "snap = {snap:?}"
    );
}

#[test]
fn tools_stay_out_of_setup_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = Store::open(&join(dir.path(), "kiki.yaml")).unwrap();
    db.save_tools(&Settings {
        github_cli: true,
        rtk: true,
        guardrails: None,
    })
    .unwrap();
    let got = db.tools().unwrap();
    assert!(got.github_cli && got.rtk && !got.empty(), "loaded = {got:?}");
    let setup = fs::read_to_string(db.file()).unwrap();
    assert!(
        !setup.contains("github_cli") && !setup.contains("rtk"),
        "setup file contains tool settings:\n{setup}"
    );
    let text = fs::read_to_string(dir.path().join("tools.yaml")).unwrap();
    assert!(
        text.contains("github_cli: true")
            && text.contains("rtk: true")
            && text.contains("Agents do not read this file"),
        "tools file =\n{text}"
    );
    db.save_tools(&Settings {
        guardrails: Some(false),
        ..Default::default()
    })
    .unwrap();
    let kept = fs::read_to_string(dir.path().join("tools.yaml")).unwrap();
    assert!(kept.contains("guardrails: false"), "tools file =\n{kept}");
    let loaded = db.tools().unwrap();
    assert!(!loaded.guardrails_on(), "guardrails stayed on");
    db.save_tools(&Settings::default()).unwrap();
    assert!(
        !exists(dir.path().join("tools.yaml")),
        "turning every tool off left the file"
    );
    let cleared = db.tools().unwrap();
    assert!(cleared.empty(), "cleared = {cleared:?}");
}

#[test]
fn setup_files_match_yaml_v3_layout() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let mut snap = db.load().unwrap();
    set_services(&mut snap, "backend", &["api", "worker"]);
    db.save(&snap).unwrap();
    db.save_team_folder(&TeamFolder {
        path: "/tmp/team".into(),
        user: "bob".into(),
        member: String::new(),
    })
    .unwrap();
    let text = fs::read_to_string(path(&root)).unwrap();
    let expected = [
        "# KIKI setup. This file is saved in the .kiki folder next to the app. Agents read it.".to_string(),
        "workspace_id: kiki".to_string(),
        "orchestrator: orca".to_string(),
        "repos:".to_string(),
        "    - kind: backend".to_string(),
        "      services:".to_string(),
        "        - api".to_string(),
        "        - worker".to_string(),
        "    - kind: website".to_string(),
        "    - kind: mobile".to_string(),
        "    - kind: document".to_string(),
        "team_folder:".to_string(),
        "    path: /tmp/team".to_string(),
        "    user: bob".to_string(),
        String::new(),
    ]
    .join("\n");
    assert_eq!(text, expected);
    let backend = fs::read_to_string(FsPath::new(&root).join(".kiki").join("repos").join("backend.yaml")).unwrap();
    assert_eq!(
        backend,
        "# Repository backend. KIKI stores this in .kiki/repos. The index is what the launch prompt tells the model to query.\nid: backend\nname: Backend\nservices:\n    - api\n    - worker\n"
    );
}

#[test]
fn dividing_a_repository_is_saved_when_no_service_is_selected() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let mut snap = db.load().unwrap();
    for repo in snap.repos.iter_mut() {
        if repo.kind == "website" {
            repo.divided = true;
        }
        if repo.kind == "document" {
            repo.divided = true;
            repo.services = vec!["notes".into()];
        }
    }
    db.save(&snap).unwrap();
    let loaded = db.load().unwrap();
    let site = loaded.repo("website");
    assert!(site.divided && site.services.is_empty() && site.uses_services());
    let docs = loaded.repo("document");
    assert!(
        docs.divided && docs.services == ["notes".to_string()],
        "document = {docs:?}"
    );
    let text = fs::read_to_string(FsPath::new(&root).join(".kiki").join("repos").join("website.yaml")).unwrap();
    assert!(text.contains("divided: true\n"), "website =\n{text}");
}

#[test]
fn a_saved_service_list_still_divides_the_repository() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let mut snap = db.load().unwrap();
    set_services(&mut snap, "backend", &["api"]);
    db.save(&snap).unwrap();
    let backend = db.load().unwrap().repo("backend");
    assert!(!backend.divided && backend.uses_services());
    assert_eq!(backend.services, ["api".to_string()]);
}

#[test]
fn setup_file_quotes_paths_like_yaml_v3() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let odd = tempfile::tempdir().unwrap();
    let folder = odd.path().join("a: b #c");
    fs::create_dir(&folder).unwrap();
    let snap = db.load().unwrap();
    db.save_repo_folder(&snap, "backend", &s(&folder)).unwrap();
    let text = fs::read_to_string(db.file()).unwrap();
    assert!(
        text.contains(&format!("      folder: '{}'\n", folder.display())),
        "setup =\n{text}"
    );
    assert_eq!(db.load().unwrap().repo("backend").folder, s(&folder));
}

#[test]
fn guide_files_are_saved_for_the_repository_and_each_service() {
    let (_dir, root, db) = open_root();
    db.ensure_layout(&root).unwrap();
    let mut snap = db.load().unwrap();
    for repo in snap.repos.iter_mut() {
        if repo.kind == "backend" {
            repo.divided = true;
            repo.services = vec!["api".into(), "web".into()];
            repo.guides = vec!["RULES.md".into(), "DESIGN.md".into()];
            repo.service_guides = vec![
                console::ServiceGuides {
                    service: "api".into(),
                    shared: true,
                    ..Default::default()
                },
                console::ServiceGuides {
                    service: "web".into(),
                    guides: vec!["WEB.md".into()],
                    ..Default::default()
                },
            ];
        }
    }
    db.save(&snap).unwrap();
    let loaded = db.load().unwrap().repo("backend");
    assert_eq!(loaded.guides, ["RULES.md".to_string(), "DESIGN.md".to_string()]);
    assert!(loaded.service_guides[0].shared && loaded.service_guides[0].service == "api");
    assert_eq!(loaded.service_guides[1].guides, ["WEB.md".to_string()]);
    let text = fs::read_to_string(FsPath::new(&root).join(".kiki").join("repos").join("backend.yaml")).unwrap();
    assert!(
        text.contains("guides:\n    - RULES.md\n    - DESIGN.md\n") && text.contains("shared: true"),
        "backend file:\n{text}"
    );
}
