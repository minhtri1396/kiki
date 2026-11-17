//! Behaviour tests against real temp directories.

use crate::tree::MAX_FILE;
use crate::*;
use std::fs;
use std::path::{Path, PathBuf};

fn p(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn members_skips_hidden_and_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let team = root.path().join("team");
    fs::create_dir_all(team.join("alice")).unwrap();
    fs::create_dir_all(team.join(".hidden")).unwrap();
    fs::write(team.join("notes.txt"), "x").unwrap();
    let outside = root.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, team.join("linked")).unwrap();
    let team = p(&team);
    let names = members(&team).unwrap();
    assert_eq!(names, vec!["alice".to_string()], "members = {names:?}");
    assert!(member(&team, "linked").is_err(), "symlink member was accepted");
    assert!(member(&team, "../outside").is_err(), "path escape was accepted");
}

#[test]
fn copy_replaces_agents_and_keeps_them_on_failure() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let member_dir: PathBuf = root.join("team").join("alice").join("requirement");
    let agents = root.join("agents").join("backend");
    fs::create_dir_all(&member_dir).unwrap();
    fs::create_dir_all(&agents).unwrap();
    const SHARED: &str = "shared agent\n";
    fs::write(member_dir.join("requirement_document.md"), SHARED).unwrap();
    fs::write(root.join("agents").join("notes.txt"), "notes").unwrap();
    let agent_dir = root.join("agents");
    copy(&p(member_dir.parent().unwrap()), &p(&agent_dir)).unwrap();
    let got = fs::read_to_string(agent_dir.join("requirement").join("requirement_document.md")).unwrap();
    assert_eq!(got, SHARED, "copied = {got:?}");
    assert!(
        fs::metadata(agent_dir.join("notes.txt")).is_err(),
        "old local file was kept"
    );
    assert!(
        fs::metadata(agent_dir.join("backend")).is_err(),
        "old local folder was kept"
    );

    let big = root.join("team").join("bob");
    fs::create_dir_all(&big).unwrap();
    fs::write(big.join("big.md"), vec![0u8; MAX_FILE as usize + 1]).unwrap();
    assert!(
        copy(&p(&big), &p(&agent_dir)).is_err(),
        "oversized member folder was copied"
    );
    fs::metadata(agent_dir.join("requirement").join("requirement_document.md")).unwrap();
}

#[test]
fn copy_rejects_overlap() {
    let root = tempfile::tempdir().unwrap();
    let team = root.path().join("team");
    let member_dir = team.join("alice");
    fs::create_dir_all(&member_dir).unwrap();
    fs::write(member_dir.join("a.md"), "a").unwrap();
    let err = copy(&p(&member_dir), &p(&team)).unwrap_err().to_string();
    assert!(err.contains("local agents folder"), "overlap err = {err}");
    fs::metadata(member_dir.join("a.md")).unwrap();
}

#[test]
fn same_and_dirs() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    fs::create_dir_all(a.join("sub")).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("sub").join("x.md"), "1").unwrap();
    fs::write(b.join("x.md"), "1").unwrap();
    assert!(!same(&p(&a), &p(&b)).unwrap());
    fs::create_dir_all(b.join("sub")).unwrap();
    fs::write(b.join("sub").join("x.md"), "1").unwrap();
    fs::remove_file(b.join("x.md")).unwrap();
    assert!(same(&p(&a), &p(&b)).unwrap());
    assert_eq!(agents_dir("/t/alice/"), "/t/alice/agents");
    assert_eq!(repos_dir("/t/alice"), "/t/alice/repos");
    assert!(dir("relative").is_err());
    assert!(valid_name("bob") && !valid_name(".x") && !valid_name("a/b"));
}
