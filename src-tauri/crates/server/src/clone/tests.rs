use std::fs;
use std::path::Path;
use std::process::Command;

use kiki_util::Ctx;

use super::folder::folder_empty;
use super::git::{git_clone, git_origin};
use super::url::valid_git_url;

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-C",
            &dir.to_string_lossy(),
            "-c",
            "user.email=kiki@example.com",
            "-c",
            "user.name=Kiki",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_output(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-C", &dir.to_string_lossy()])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    text
}

fn write_repo_file(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn exists(path: impl AsRef<Path>) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn folder_empty_ignores_index_setup() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(folder_empty(&s(dir)).unwrap(), "empty dir");

    fs::create_dir(dir.join(".codegraph")).unwrap();
    fs::write(dir.join(".codegraph").join("db"), "x").unwrap();
    fs::create_dir_all(dir.join(".obsidian")).unwrap();
    fs::write(dir.join(".obsidian").join("app.json"), "{}\n").unwrap();
    fs::write(dir.join(".DS_Store"), "x").unwrap();
    assert!(folder_empty(&s(dir)).unwrap(), "setup only");

    fs::write(dir.join("README.md"), "hi\n").unwrap();
    assert!(!folder_empty(&s(dir)).unwrap(), "with a file");
}

#[test]
fn git_origin_reads_the_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert_eq!(git_origin(&s(dir)), "", "plain folder");
    run_git(dir, &["init", "-b", "main"]);
    assert_eq!(git_origin(&s(dir)), "", "no remote");
    run_git(dir, &["remote", "add", "origin", "https://example.com/app.git"]);
    assert_eq!(git_origin(&s(dir)), "https://example.com/app.git", "origin");
    assert_eq!(git_origin(&s(&dir.join("missing"))), "", "missing");
}

#[test]
fn folder_empty_missing_path() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(folder_empty(&s(&tmp.path().join("missing"))).unwrap());
}

#[test]
fn git_clone_replaces_index_setup() {
    let src_tmp = tempfile::tempdir().unwrap();
    let src = src_tmp.path();
    run_git(src, &["init", "-b", "main"]);
    fs::write(src.join("README.md"), "hello\n").unwrap();
    run_git(src, &["add", "README.md"]);
    run_git(src, &["commit", "-m", "init"]);

    let dest_tmp = tempfile::tempdir().unwrap();
    let dest = dest_tmp.path();
    let marker = dest.join(".obsidian").join("app.json");
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    fs::write(&marker, "{}\n").unwrap();
    fs::create_dir(dest.join(".codegraph")).unwrap();
    assert!(folder_empty(&s(dest)).unwrap(), "before clone");

    git_clone(&Ctx::background(), &s(src), &s(dest), "").unwrap();
    assert_eq!(fs::read_to_string(dest.join("README.md")).unwrap(), "hello\n");
    assert!(exists(dest.join(".git")));
    assert!(!exists(&marker), "obsidian setup was left in the cloned folder");
    assert!(
        !exists(dest.join(".codegraph")),
        "codegraph setup was left in the cloned folder"
    );
}

#[test]
fn git_clone_keeps_setup_when_clone_fails() {
    let dest_tmp = tempfile::tempdir().unwrap();
    let dest = dest_tmp.path();
    let marker = dest.join(".obsidian").join("app.json");
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    fs::write(&marker, "{}\n").unwrap();
    let missing = tempfile::tempdir().unwrap();
    let err = git_clone(
        &Ctx::background(),
        &s(&missing.path().join("missing.git")),
        &s(dest),
        "",
    );
    assert!(err.is_err(), "expected clone to fail");
    assert!(exists(&marker), "setup was removed after a failed clone");
}

fn sparse_source() -> tempfile::TempDir {
    let src_tmp = tempfile::tempdir().unwrap();
    let src = src_tmp.path();
    run_git(src, &["init", "-b", "main"]);
    write_repo_file(src, "README.md", "root\n");
    write_repo_file(src, "product/kiki/note.md", "hello\n");
    write_repo_file(src, "other/secret.md", "secret\n");
    run_git(src, &["add", "."]);
    run_git(src, &["commit", "-m", "init"]);
    src_tmp
}

#[test]
fn git_clone_sparse_checks_out_one_folder() {
    let src_tmp = sparse_source();
    let dest_tmp = tempfile::tempdir().unwrap();
    let dest = dest_tmp.path();
    git_clone(&Ctx::background(), &s(src_tmp.path()), &s(dest), "product/kiki").unwrap();
    assert!(exists(dest.join("product").join("kiki").join("note.md")));
    assert!(exists(dest.join("README.md")));
    assert!(
        !exists(dest.join("other").join("secret.md")),
        "other folder was checked out"
    );

    fs::write(dest.join("product").join("kiki").join("note.md"), "edited\n").unwrap();
    run_git(dest, &["add", "product/kiki/note.md"]);
    run_git(dest, &["commit", "-m", "edit"]);
    let tree = git_output(dest, &["ls-tree", "-r", "--name-only", "HEAD"]);
    for name in ["README.md", "product/kiki/note.md", "other/secret.md"] {
        assert!(tree.contains(name), "commit tree missing {name}:\n{tree}");
    }
    let status = git_output(dest, &["status", "--porcelain"]);
    assert!(!status.contains("other/"), "status touches the other folder:\n{status}");
}

#[test]
fn git_clone_sparse_pull_keeps_the_other_folder_out() {
    let src_tmp = sparse_source();
    let src = src_tmp.path();
    let dest_tmp = tempfile::tempdir().unwrap();
    let dest = dest_tmp.path();
    git_clone(&Ctx::background(), &s(src), &s(dest), "product/kiki").unwrap();
    write_repo_file(src, "product/kiki/note.md", "from remote\n");
    write_repo_file(src, "other/secret.md", "still secret\n");
    run_git(src, &["add", "."]);
    run_git(src, &["commit", "-m", "remote"]);
    run_git(dest, &["pull"]);

    let got = fs::read_to_string(dest.join("product").join("kiki").join("note.md")).unwrap();
    assert_eq!(got, "from remote\n");
    assert!(
        !exists(dest.join("other").join("secret.md")),
        "pull checked out the other folder"
    );
    let secret = git_output(dest, &["show", "HEAD:other/secret.md"]);
    assert_eq!(secret, "still secret\n");
}

#[test]
fn git_clone_sparse_rejects_a_missing_folder() {
    let src_tmp = tempfile::tempdir().unwrap();
    let src = src_tmp.path();
    run_git(src, &["init", "-b", "main"]);
    write_repo_file(src, "README.md", "hello\n");
    run_git(src, &["add", "README.md"]);
    run_git(src, &["commit", "-m", "init"]);

    let dest_tmp = tempfile::tempdir().unwrap();
    let dest = dest_tmp.path();
    let marker = dest.join(".obsidian").join("app.json");
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    fs::write(&marker, "{}\n").unwrap();
    let err = git_clone(&Ctx::background(), &s(src), &s(dest), "missing/folder").unwrap_err();
    assert!(format!("{err:#}").contains("not in this repository"), "{err:#}");
    assert!(exists(&marker), "setup was removed after a failed clone");
    assert!(
        !exists(dest.join("README.md")),
        "a failed sparse clone was left in the folder"
    );
}

#[test]
fn valid_git_url_cases() {
    for ok in [
        "https://example.com/app.git",
        "http://example.com/app.git",
        "ssh://git@example.com/app.git",
        "git://example.com/app.git",
        "git@github.com:acme/app.git",
        "file:///tmp/app",
    ] {
        assert!(valid_git_url(ok), "{ok}");
    }
    for bad in [
        "",
        "-x",
        "a b",
        "/tmp/app",
        "git@host",
        "git@:p",
        "https://",
        "ftp://example.com/x",
        "file:foo",
    ] {
        assert!(!valid_git_url(bad), "{bad}");
    }
}
