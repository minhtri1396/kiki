//! Unit tests of the shared repository format, the merge of shared repositories and the small enums.

use kiki_console as console;

use crate::team::shared_repo::{list_shared_repos, merge_shared, parse_shared_repo, publish_repos, SharedRepo};
use crate::team::views::{same_repo_views, TeamRepoView};
use crate::team::{Direction, Part};

use super::*;

/// The hand-written emitter this module used before it adopted `kiki_yaml`. It stays here so the new output is
/// compared with the old bytes.
mod legacy {
    use crate::team::shared_repo::SharedRepo;

    fn scalar(s: &str) -> String {
        let lower = s.to_ascii_lowercase();
        let reserved = matches!(
            lower.as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n" | "null" | "~"
        );
        let first_ok = s
            .chars()
            .next()
            .map(|c| c.is_ascii_alphanumeric() || c == '_')
            .unwrap_or(false);
        let chars_ok = s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'));
        let numeric = s.replace('_', "").parse::<f64>().is_ok()
            || lower.starts_with("0x")
            || lower.starts_with("0o")
            || lower.starts_with("0b");
        if first_ok && chars_ok && !reserved && !numeric {
            s.to_string()
        } else {
            serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
        }
    }

    pub fn canonical(repo: &SharedRepo) -> String {
        let mut out = format!("id: {}\n", scalar(&repo.id));
        if repo.services.is_empty() {
            out.push_str("services: []\n");
        } else {
            out.push_str("services:\n");
            for service in &repo.services {
                out.push_str(&format!("    - {}\n", scalar(service)));
            }
        }
        if !repo.index.is_empty() {
            out.push_str(&format!("index: {}\n", scalar(&repo.index)));
        }
        if !repo.sparse.is_empty() {
            out.push_str(&format!("sparse: {}\n", scalar(&repo.sparse)));
        }
        out
    }
}

fn shared(id: &str, services: &[&str], index: &str, sparse: &str) -> SharedRepo {
    SharedRepo {
        id: id.into(),
        name: String::new(),
        divided: false,
        services: services.iter().map(|s| s.to_string()).collect(),
        index: index.into(),
        sparse: sparse.into(),
    }
}

#[test]
fn canonical_text_has_the_layout_of_the_files_already_shared() {
    let repo = shared("backend", &["api", "worker"], "codegraph", "product/requirements");
    assert_eq!(
        repo.canonical(),
        "id: backend\nservices:\n    - api\n    - worker\nindex: codegraph\nsparse: product/requirements\n"
    );
    assert_eq!(
        shared("document", &[], "", "").canonical(),
        "id: document\nservices: []\n"
    );
}

#[test]
fn canonical_text_matches_the_old_emitter_for_ordinary_values() {
    let services = ["api", "web-app", "svc_1", "payments.v2", "a/b", "Gateway", "x1"];
    let cases = [
        shared("backend", &[], "", ""),
        shared("backend", &services, "obsidian", "product/requirements"),
        shared("mobile", &["ios", "android"], "codegraph", "apps/mobile_app"),
        shared("payments_2", &["on-call", "yes_sir"], "", "docs"),
    ];
    for repo in &cases {
        assert_eq!(repo.canonical(), legacy::canonical(repo), "repo = {repo:?}");
    }
}

#[test]
fn canonical_text_quotes_values_that_would_read_back_as_something_else() {
    // Both emitters quote names that YAML would read as a bool, null or number.
    for service in ["true", "No", "null", "~", "12", "1.5", "0x1F", "y"] {
        let repo = shared("backend", &[service], "", "");
        assert_eq!(repo.canonical(), legacy::canonical(&repo), "service = {service}");
        assert!(
            repo.canonical().contains('"'),
            "service {service} stayed plain: {}",
            repo.canonical()
        );
    }
}

#[test]
fn canonical_text_differs_from_the_old_emitter_only_in_scalar_quoting() {
    // kiki_yaml follows yaml.v3: text that is safe plain YAML stays plain (the old emitter always quoted it).
    let repo = shared("backend", &["my service", "-lead"], "", "docs/my folder");
    assert_eq!(
        repo.canonical(),
        "id: backend\nservices:\n    - my service\n    - -lead\nsparse: docs/my folder\n"
    );
    assert_eq!(
        legacy::canonical(&repo),
        "id: backend\nservices:\n    - \"my service\"\n    - \"-lead\"\nsparse: \"docs/my folder\"\n"
    );
}

#[test]
fn a_different_display_name_changes_the_shared_hash() {
    let mut local = shared("backend", &["api"], "codegraph", "");
    local.name = "Backend".into();
    let mut team = shared("backend", &["api"], "codegraph", "");
    team.name = "Services".into();
    assert_ne!(local.canonical(), team.canonical());
    assert!(local.canonical().contains("name: Backend\n"), "{}", local.canonical());
    let view = |repo: &SharedRepo| TeamRepoView {
        id: repo.id.clone(),
        name: repo.name.clone(),
        hash: repo.canonical(),
    };
    assert!(!same_repo_views(&[view(&local)], &[view(&team)]));
    let unnamed = shared("backend", &["api"], "codegraph", "");
    assert_ne!(local.canonical(), unnamed.canonical());
    let back = parse_shared_repo(local.canonical().as_bytes(), "backend.yaml")
        .unwrap()
        .unwrap();
    assert_eq!(back, local);
}

#[test]
fn canonical_text_reads_back_as_the_same_repository() {
    let repo = shared(
        "backend",
        &["my service", "true", "a: b", "# note"],
        "obsidian",
        "docs/my folder",
    );
    let back = parse_shared_repo(repo.canonical().as_bytes(), "backend.yaml")
        .unwrap()
        .unwrap();
    assert_eq!(back, repo);
}

#[test]
fn parse_drops_what_is_not_shared() {
    let raw =
        b"id: backend\nfolder: /other/machine/backend\nservices:\n  - ' web '\n  - ''\nindex: git\nsparse: /docs/\n";
    let repo = parse_shared_repo(raw, "backend.yaml").unwrap().unwrap();
    assert_eq!(repo, shared("backend", &["web"], "", "docs"));
    let document = parse_shared_repo(b"services:\n  - web\n", "document.yaml")
        .unwrap()
        .unwrap();
    assert!(document.services.is_empty());
    assert_eq!(
        parse_shared_repo(b"", "empty.yaml").unwrap(),
        Some(shared("empty", &[], "", ""))
    );
}

#[test]
fn parse_skips_names_that_are_not_repository_ids_and_reports_bad_files() {
    assert_eq!(parse_shared_repo(b"id: x\n", "Not-An-Id.yaml").unwrap(), None);
    let err = parse_shared_repo(b"services: [\n", "backend.yaml").unwrap_err();
    assert!(err.to_string().starts_with("read backend.yaml: "), "error = {err}");
    let err = parse_shared_repo(b"sparse: ../x\n", "backend.yaml").unwrap_err();
    assert!(err.to_string().starts_with("backend: "), "error = {err}");
}

#[test]
fn published_repositories_are_listed_in_id_order_with_the_same_text() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");
    mkdirs(&src);
    write(&src.join("website.yaml"), "services:\n  - web\n");
    write(&src.join("backend.yaml"), "services:\n  - api\nindex: codegraph\n");
    write(&src.join("notes.txt"), "ignored");
    write(&src.join(".hidden.yaml"), "services:\n  - x\n");
    publish_repos(s(&src), s(&dest)).unwrap();
    let listed = list_shared_repos(s(&dest)).unwrap();
    assert_eq!(
        listed,
        vec![
            shared("backend", &["api"], "codegraph", ""),
            shared("website", &["web"], "", "")
        ]
    );
    assert_eq!(
        fs::read_to_string(dest.join("backend.yaml")).unwrap(),
        "id: backend\nservices:\n    - api\nindex: codegraph\n"
    );
}

#[test]
fn publishing_without_repository_files_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = publish_repos(s(&dir.path().join("missing")), s(&dir.path().join("dest"))).unwrap_err();
    assert_eq!(err.to_string(), "This app has no repository files to copy.");
}

fn local(kind: &str, folder: &str) -> console::Repo {
    console::Repo {
        kind: kind.into(),
        folder: folder.into(),
        ..Default::default()
    }
}

#[test]
fn merging_keeps_checkout_folders_and_orders_extras_after_the_built_in_kinds() {
    let repos = vec![local("backend", "/work/backend"), local("search", "/work/search")];
    let files = vec![
        shared("zeta", &["z"], "", ""),
        shared("backend", &["api"], "codegraph", ""),
        shared("search", &["q"], "", "docs"),
        shared("alpha", &[], "obsidian", ""),
        shared("document", &["ignored"], "", ""),
    ];
    let (merged, added) = merge_shared(&repos, files);
    let kinds: Vec<&str> = merged.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, ["backend", "document", "alpha", "search", "zeta"]);
    assert_eq!(added, ["document", "alpha", "zeta"]);
    let by = |kind: &str| merged.iter().find(|r| r.kind == kind).unwrap().clone();
    assert_eq!(
        (
            by("backend").folder.as_str(),
            by("backend").services,
            by("backend").index.as_str()
        ),
        ("/work/backend", vec!["api".to_string()], "codegraph")
    );
    assert_eq!(
        (by("search").folder.as_str(), by("search").sparse.as_str()),
        ("/work/search", "docs")
    );
    assert!(by("document").services.is_empty());
    assert_eq!(by("document").folder, "");
    assert_eq!(by("zeta").folder, "");
    assert!(merged
        .iter()
        .all(|repo| repo.kind != "website" && repo.kind != "mobile"));
}

fn view(id: &str, hash: &str) -> TeamRepoView {
    TeamRepoView {
        id: id.into(),
        name: id.into(),
        hash: hash.into(),
    }
}

#[test]
fn repo_listings_are_the_same_only_with_equal_ids_and_hashes() {
    let left = [view("backend", "a"), view("website", "b")];
    assert!(same_repo_views(&left, &[view("website", "b"), view("backend", "a")]));
    assert!(!same_repo_views(&left, &[view("backend", "a")]));
    assert!(!same_repo_views(
        &left,
        &[view("backend", "a"), view("website", "changed")]
    ));
    assert!(!same_repo_views(&left, &[view("backend", "a"), view("mobile", "b")]));
}

#[test]
fn directions_and_parts_parse_only_their_exact_names() {
    assert_eq!(Direction::parse("pull"), Some(Direction::Pull));
    assert_eq!(Direction::parse("push"), Some(Direction::Push));
    assert_eq!(Direction::parse("Pull"), None);
    assert_eq!(Part::parse("", Part::Repos), Some(Part::Repos));
    assert_eq!(Part::parse("agents", Part::Repos), Some(Part::Agents));
    assert_eq!(Part::parse("repos", Part::Agents), Some(Part::Repos));
    assert_eq!(Part::parse("all", Part::Agents), None);
}

#[test]
fn the_repository_folder_sits_next_to_the_setup_file() {
    let app = new_team_test_server();
    assert_eq!(app.repo_dir().unwrap(), s(&app.root.join("repos")));
}
