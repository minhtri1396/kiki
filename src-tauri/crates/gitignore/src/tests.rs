//! Behaviour tests against real temp directories and `git check-ignore`.

use crate::rules::{HEADER_COMMENT, INDEX_HEADER};
use crate::worktree::find_work_tree;
use crate::{app_entries, ensure, index, Entry};
use std::path::Path;
use std::process::Command;

fn join(a: &Path, b: &str) -> String {
    a.join(b).to_string_lossy().into_owned()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn e(path: String) -> Entry {
    Entry { path, dir: false }
}

fn d(path: String) -> Entry {
    Entry { path, dir: true }
}

fn exists(p: &str) -> bool {
    Path::new(p).exists()
}

fn git_ignored(root: &Path, rel: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "-q", rel])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn ensure_leaves_non_repo_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = s(tmp.path());
    if find_work_tree(&dir).unwrap().is_some() {
        // temp dir is inside a git work tree
        return;
    }
    let path = ensure(&dir, &[e(join(tmp.path(), "kiki.yaml"))]).unwrap();
    assert_eq!(path, "", "path = {path:?}");
    assert!(
        !exists(&join(tmp.path(), ".gitignore")),
        "created .gitignore outside a work tree"
    );
}

#[test]
fn ensure_adds_only_missing_app_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join(".git")).unwrap();
    let ignore = join(root, ".gitignore");
    let existing = "# keep\nkiki.yaml\n/backend/\n";
    std::fs::write(&ignore, existing).unwrap();
    let entries = app_entries(
        &s(root),
        &join(root, "agents"),
        &join(root, "kiki.yaml"),
        &join(root, "kiki.db"),
    );
    let path = ensure(&s(root), &entries).unwrap();
    assert_eq!(path, ignore);
    let got = std::fs::read_to_string(&ignore).unwrap();
    let want = format!(
        "{existing}\n{HEADER_COMMENT}\n{}",
        "/kiki.yaml.tmp
/mcp.yaml
/mcp.yaml.tmp
/tools.yaml
/tools.yaml.tmp
/atlassian.yaml
/atlassian.yaml.tmp
/kiki.db
/kiki.db-shm
/kiki.db-wal
/kiki.db-journal
/website/
/mobile/
/docs/
/agents/
"
    );
    assert_eq!(got, want, "gitignore:\n{got}\nwant:\n{want}");
    let again = ensure(&s(root), &entries).unwrap();
    assert_eq!(again, "", "second ensure rewrote .gitignore");
    let got = std::fs::read_to_string(&ignore).unwrap();
    assert_eq!(got, want, "second ensure changed the file");
}

#[test]
fn ensure_uses_git_root_for_nested_app() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join(".git")).unwrap();
    let app = root.join("apps").join("kiki");
    std::fs::create_dir_all(&app).unwrap();
    let ignore = join(root, ".gitignore");
    std::fs::write(&ignore, "kiki.db\n").unwrap();
    let path = ensure(&s(&app), &[e(join(&app, "kiki.db")), d(join(&app, "docs"))]).unwrap();
    assert_eq!(path, ignore);
    let got = std::fs::read_to_string(&ignore).unwrap();
    let want = format!("kiki.db\n\n{HEADER_COMMENT}\n/apps/kiki/docs/\n");
    assert_eq!(got, want, "gitignore:\n{got}\nwant:\n{want}");
    assert!(
        !exists(&join(&app, ".gitignore")),
        "wrote .gitignore next to the app instead of the work tree root"
    );
}

#[test]
fn ensure_skips_paths_outside_work_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join(".git"), "gitdir: /nowhere\n").unwrap();
    let other = tempfile::tempdir().unwrap();
    let outside = join(other.path(), "agents");
    let path = ensure(&s(root), &[d(outside)]).unwrap();
    assert_eq!(path, "", "path = {path:?}");
    assert!(
        !exists(&join(root, ".gitignore")),
        "created .gitignore for a path outside the work tree"
    );
}

#[test]
fn ensure_replaces_unignored_path() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join(".git")).unwrap();
    let ignore = join(root, ".gitignore");
    std::fs::write(&ignore, "kiki.yaml\n!kiki.yaml\n").unwrap();
    ensure(&s(root), &[e(join(root, "kiki.yaml"))]).unwrap();
    let got = std::fs::read_to_string(&ignore).unwrap();
    assert!(got.ends_with("\n/kiki.yaml\n"), "gitignore:\n{got}");
}

#[test]
fn ensure_does_not_repeat_a_path_inside_an_ignored_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join(".git")).unwrap();
    ensure(
        &s(root),
        &[d(join(root, "docs")), d(s(&root.join("docs").join("vault")))],
    )
    .unwrap();
    let got = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(!got.contains("vault"), "nested path was listed separately:\n{got}");
}

#[test]
fn ensure_keeps_crlf() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join(".git")).unwrap();
    let ignore = join(root, ".gitignore");
    std::fs::write(&ignore, "keep\r\n").unwrap();
    ensure(&s(root), &[d(join(root, "agents"))]).unwrap();
    let got = std::fs::read_to_string(&ignore).unwrap();
    let want = "keep\r\n\r\n# KIKI generated files\r\n/agents/\r\n";
    assert_eq!(got, want, "gitignore:\n{got:?}\nwant:\n{want:?}");
}

#[test]
fn index_ignores_codegraph_and_obsidian() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git init: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    index(&s(root), &[".codegraph"]).unwrap();
    index(&s(root), &[".obsidian"]).unwrap();
    let got = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    let want = format!("{INDEX_HEADER}\n/.codegraph/\n/.obsidian/\n");
    assert_eq!(got, want, "gitignore:\n{got}\nwant:\n{want}");
    std::fs::create_dir_all(root.join(".codegraph")).unwrap();
    std::fs::write(root.join(".codegraph").join("db"), "x").unwrap();
    std::fs::create_dir_all(root.join(".obsidian")).unwrap();
    std::fs::write(root.join(".obsidian").join("app.json"), "{}\n").unwrap();
    std::fs::write(root.join("README.md"), "keep\n").unwrap();
    assert!(git_ignored(root, ".codegraph/db"), ".codegraph/db should be ignored");
    assert!(
        git_ignored(root, ".obsidian/app.json"),
        ".obsidian/app.json should be ignored"
    );
    assert!(
        !git_ignored(root, "README.md"),
        "README.md should stay available to commit"
    );
    let again = index(&s(root), &[".codegraph"]).unwrap();
    assert_eq!(again, "", "second index setup rewrote .gitignore");
}

#[test]
fn index_uses_the_repository_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path();
    std::fs::create_dir(parent.join(".git")).unwrap();
    let repo = parent.join("backend");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir(repo.join(".git")).unwrap();
    let path = index(&s(&repo), &[".codegraph"]).unwrap();
    assert_eq!(path, join(&repo, ".gitignore"));
    assert!(!exists(&join(parent, ".gitignore")), "parent .gitignore was written");
}

#[test]
fn index_uses_parent_work_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path();
    std::fs::create_dir(parent.join(".git")).unwrap();
    let repo = parent.join("docs");
    std::fs::create_dir_all(&repo).unwrap();
    index(&s(&repo), &[".obsidian"]).unwrap();
    let got = std::fs::read_to_string(parent.join(".gitignore")).unwrap();
    assert_eq!(got, format!("{INDEX_HEADER}\n/docs/.obsidian/\n"), "gitignore:\n{got}");
}

#[test]
fn index_leaves_existing_rule() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join(".git")).unwrap();
    let ignore = join(root, ".gitignore");
    let existing = ".codegraph/\n";
    std::fs::write(&ignore, existing).unwrap();
    let path = index(&s(root), &[".codegraph"]).unwrap();
    assert_eq!(path, "", "existing rule was treated as missing");
    let got = std::fs::read_to_string(&ignore).unwrap();
    assert_eq!(got, existing, "gitignore:\n{got}");
}

#[test]
fn index_leaves_non_repo_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = s(tmp.path());
    if find_work_tree(&dir).unwrap().is_some() {
        // temp dir is inside a git work tree
        return;
    }
    let path = index(&dir, &[".codegraph"]).unwrap();
    assert_eq!(path, "", "path = {path:?}");
    assert!(
        !exists(&join(tmp.path(), ".gitignore")),
        "created .gitignore outside a work tree"
    );
}

#[test]
fn anchored_agents_pattern_does_not_ignore_internal_agents() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = Command::new("git").arg("init").current_dir(root).output().unwrap();
    assert!(
        out.status.success(),
        "git init: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let internal = root
        .join("internal")
        .join("agents")
        .join("requirement")
        .join("planner.md");
    std::fs::create_dir_all(internal.parent().unwrap()).unwrap();
    std::fs::write(&internal, "x").unwrap();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(root.join("agents").join("note.md"), "x").unwrap();
    ensure(&s(root), &[d(join(root, "agents"))]).unwrap();
    assert!(git_ignored(root, "agents/note.md"), "agents/note.md should be ignored");
    assert!(
        !git_ignored(root, "internal/agents/requirement/planner.md"),
        "internal/agents should stay tracked"
    );
}
