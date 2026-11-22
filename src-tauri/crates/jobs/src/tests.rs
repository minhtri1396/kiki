//! Behaviour tests of the manager: setup, checks and the background runner.

use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use kiki_util::Ctx;

use crate::index::{self, Launch};
use crate::{Check, JobStatus, Manager, Report, Target};

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn not_found_look_path(manager: &mut Manager) {
    manager.env.look_path = Arc::new(|name: &str| Err(anyhow!("command {name} was not found on PATH")));
}

fn set_last(manager: &Manager, repos: &[(&str, Check)]) {
    let mut st = manager.state.lock().unwrap();
    st.last.repos = repos.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
}

fn target(kind: &str, folder: &str, index: &str) -> Target {
    Target {
        kind: kind.to_string(),
        folder: folder.to_string(),
        index: index.to_string(),
    }
}

#[test]
fn start_codegraph_writes_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let repo = root.join("backend");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir(repo.join(".git")).unwrap();
    let mut manager = Manager::new();
    not_found_look_path(&mut manager);
    let err = manager
        .start_codegraph(&path_str(root), "backend", &path_str(&repo))
        .expect_err("expected an error");
    assert!(err.to_string().contains("codegraph"), "err = {err}");
    let got = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert!(
        got.contains("/.codegraph/") && got.contains("/.obsidian/"),
        "gitignore:\n{got}"
    );
}

#[test]
fn check_one_keeps_other_results() {
    let mut manager = Manager::new();
    not_found_look_path(&mut manager);
    set_last(
        &manager,
        &[(
            "backend",
            Check {
                ready: true,
                summary: "keep".into(),
                checked_at: "earlier".into(),
                ..Default::default()
            },
        )],
    );
    let tmp = tempfile::tempdir().unwrap();
    let dir = path_str(tmp.path());
    let report = manager
        .check_one(
            &Ctx::background(),
            &dir,
            &[target("document", &dir, "obsidian")],
            "document",
        )
        .unwrap();
    assert_eq!(
        report.repos["backend"].summary, "keep",
        "backend = {:?}",
        report.repos["backend"]
    );
    let document = report.repos.get("document").cloned().unwrap_or_default();
    assert!(
        !document.checked_at.is_empty() && document.summary.contains("obsidian"),
        "document = {document:?}"
    );
    assert!(
        manager.check_one(&Ctx::background(), &dir, &[], "document").is_err(),
        "expected unknown repository"
    );
}

#[test]
fn refresh_folders_reports_the_index_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let docs = root.join("docs");
    let backend = root.join("backend");
    std::fs::create_dir_all(docs.join(".codegraph")).unwrap();
    std::fs::create_dir_all(&backend).unwrap();
    let manager = Manager::new();
    set_last(
        &manager,
        &[(
            "website",
            Check {
                summary: "keep".into(),
                checked_at: "earlier".into(),
                ..Default::default()
            },
        )],
    );
    let report = manager
        .refresh_folders(
            &[
                target("document", &path_str(&docs), "codegraph"),
                target("backend", &path_str(&backend), "codegraph"),
                target("website", &path_str(root), "obsidian"),
            ],
            "codegraph",
        )
        .unwrap();
    let document = &report.repos["document"];
    assert!(
        document.dir_exists && document.summary == ".codegraph exists",
        "document = {document:?}"
    );
    let backend = &report.repos["backend"];
    assert!(
        !backend.dir_exists && backend.summary == ".codegraph is missing",
        "backend = {backend:?}"
    );
    let website = &report.repos["website"];
    assert_eq!(website.summary, "keep", "website = {website:?}");
    assert!(manager.refresh_folders(&[], "git").is_err(), "expected unknown index");
}

#[test]
fn check_uses_the_saved_index() {
    let mut manager = Manager::new();
    not_found_look_path(&mut manager);
    let tmp = tempfile::tempdir().unwrap();
    let dir = path_str(tmp.path());
    let report: Report = manager
        .check(
            &Ctx::background(),
            &dir,
            &[target("document", &dir, "codegraph"), target("backend", &dir, "")],
        )
        .unwrap();
    let document = report.repos.get("document").cloned().unwrap_or_default();
    assert!(document.summary.contains("codegraph"), "document = {document:?}");
    let backend = report.repos.get("backend").cloned().unwrap_or_default();
    assert_eq!(
        backend.summary, "No index is set for this repository",
        "backend = {backend:?}"
    );
}

#[test]
fn obsidian_check_retries_when_the_vault_command_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::create_dir_all(dir.join(".obsidian")).unwrap();
    let abs = std::path::absolute(dir).unwrap();
    let abs = path_str(&abs);
    let count = path_str(&dir.join("count"));
    let script = path_str(&dir.join("obsidian"));
    let body = format!(
        "#!/bin/sh\n\
n=0\n\
if [ -f {count:?} ]; then n=$(cat {count:?}); fi\n\
n=$((n+1))\n\
printf '%s' \"$n\" > {count:?}\n\
if [ \"$n\" -eq 1 ]; then\n\
  printf '%s\\n' 'Command \"vault\" not found'\n\
  exit 1\n\
fi\n\
printf '%s\\n' {abs:?}\n"
    );
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(&script)
            .unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }
    let mut manager = Manager::new();
    manager.env.obsidian_retry_delay = Duration::from_millis(1);
    let script_for_lookup = script.clone();
    manager.env.look_path = Arc::new(move |name: &str| {
        if name != "obsidian" {
            return Err(anyhow!("command {name} was not found on PATH"));
        }
        Ok(script_for_lookup.clone())
    });
    let result = manager.check_one(
        &Ctx::background(),
        &path_str(dir),
        &[target("document", &path_str(dir), "obsidian")],
        "document",
    );
    let report = result.unwrap();
    let got = report.repos["document"].clone();
    assert!(got.ready && got.summary.contains(&abs), "document = {got:?}");
    let tries = std::fs::read_to_string(&count).unwrap();
    assert_eq!(tries, "2", "tries = {tries}");
}

#[test]
fn obsidian_check_falls_back_when_the_named_vault_hangs() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("kiki-vault-hang");
    std::fs::create_dir_all(dir.join(".obsidian")).unwrap();
    let abs = std::path::absolute(&dir).unwrap();
    let abs = path_str(&abs);
    let script = path_str(&dir.join("obsidian"));
    let body =
        format!("#!/bin/sh\ncase \"$1\" in\n  vault=*) exec sleep 30 ;;\n  *) printf '%s\\n' {abs:?} ;;\nesac\n");
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(&script)
            .unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }
    let mut manager = Manager::new();
    manager.env.obsidian_call_timeout = Duration::from_millis(200);
    let script_for_lookup = script.clone();
    manager.env.look_path = Arc::new(move |name: &str| {
        if name != "obsidian" {
            return Err(anyhow!("command {name} was not found on PATH"));
        }
        Ok(script_for_lookup.clone())
    });
    let report = manager
        .check_one(
            &Ctx::background(),
            &path_str(tmp.path()),
            &[target("document", &abs, "obsidian")],
            "document",
        )
        .unwrap();
    let got = report.repos["document"].clone();
    assert!(got.ready && got.summary.contains(&abs), "document = {got:?}");
}

#[test]
fn start_runs_in_background_and_cancels() {
    let tmp = tempfile::tempdir().unwrap();
    let root = path_str(tmp.path());
    let mut manager = Manager::new();
    manager.env.look_path =
        Arc::new(|name: &str| Ok(format!("/bin/{}", if name == "codegraph" { "sh" } else { name })));
    // `codegraph init <folder>` runs as `sh init <folder>`: a failing job ends with status "error".
    let job = manager.start_codegraph(&root, "backend", &root).unwrap();
    assert_eq!(job.status, JobStatus::Running);
    for _ in 0..200 {
        if manager.get(&job.id).unwrap().status != JobStatus::Running {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let done = manager.get(&job.id).unwrap();
    assert_eq!(done.status, JobStatus::Error, "{done:?}");
    assert_eq!(manager.list().len(), 1);
    assert!(!manager.cancel(&job.id));

    // a long-running job can be cancelled
    manager.env.look_path = Arc::new(|_: &str| Ok("/bin/sleep".to_string()));
    let launch = Launch {
        dir: root.clone(),
        args: vec!["sleep".into(), "30".into()],
        note: String::new(),
    };
    let job = manager.start(index::codegraph(), "website", &launch).unwrap();
    assert!(manager.cancel(&job.id));
    for _ in 0..300 {
        if manager.get(&job.id).unwrap().status != JobStatus::Running {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let done = manager.get(&job.id).unwrap();
    assert_eq!(done.status, JobStatus::Cancelled, "{done:?}");
    assert_eq!(done.error, "cancelled");
}

fn wait_finished(manager: &Manager, id: &str) -> crate::Job {
    for _ in 0..300 {
        let job = manager.get(id).unwrap();
        if job.status != JobStatus::Running {
            return job;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("job {id} did not finish");
}

#[test]
fn obsidian_job_fails_when_the_log_says_vault_not_found() {
    let mut manager = Manager::new();
    manager.env.look_path = Arc::new(|_: &str| Ok("/bin/echo".to_string()));
    let launch = Launch {
        dir: String::new(),
        args: vec!["echo".into(), "Vault not found.".into()],
        note: "note".into(),
    };
    let job = manager.start(index::obsidian(), "backend", &launch).unwrap();
    assert_eq!(job.kind, "obsidian");
    let done = wait_finished(&manager, &job.id);
    assert_eq!(done.status, JobStatus::Error, "{done:?}");
    assert_eq!(done.error, "Vault not found.");
    assert!(done.log.starts_with("note\nVault not found."), "log = {:?}", done.log);

    let launch = Launch {
        dir: String::new(),
        args: vec!["echo".into(), "fine".into()],
        note: String::new(),
    };
    let job = manager.start(index::codegraph(), "backend", &launch).unwrap();
    let done = wait_finished(&manager, &job.id);
    assert_eq!(done.status, JobStatus::Ok, "{done:?}");
    assert!(done.error.is_empty() && !done.finished_at.is_empty());
}

#[test]
fn job_status_serializes_as_lowercase_words() {
    let json = serde_json::to_string(&JobStatus::Cancelled).unwrap();
    assert_eq!(json, "\"cancelled\"");
    assert_eq!(serde_json::from_str::<JobStatus>("\"ok\"").unwrap(), JobStatus::Ok);
    assert_eq!(JobStatus::Running.to_string(), "running");
}

#[test]
fn starting_twice_returns_the_running_job() {
    let mut manager = Manager::new();
    manager.env.look_path = Arc::new(|_: &str| Ok("/bin/sleep".to_string()));
    let launch = Launch {
        dir: String::new(),
        args: vec!["sleep".into(), "30".into()],
        note: String::new(),
    };
    let first = manager.start(index::codegraph(), "backend", &launch).unwrap();
    let second = manager.start(index::codegraph(), "backend", &launch).unwrap();
    assert_eq!(first.id, second.id);
    assert!(manager.cancel(&first.id));
    wait_finished(&manager, &first.id);
}
