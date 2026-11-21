use super::fsutil::{same_path, write_file};
use super::open::open_vault_code;
use super::*;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;

type RelaunchFn = Box<dyn Fn(&str) -> Result<()> + Send + Sync>;

/// A test environment: fixed paths and a recorded relaunch callback.
#[derive(Default)]
struct TestEnv {
    config_file: String,
    cli_candidates: Vec<String>,
    link_dir: String,
    on_relaunch: Option<RelaunchFn>,
}

impl ObsidianEnv for TestEnv {
    fn config_file(&self) -> String {
        self.config_file.clone()
    }
    fn relaunch(&self, folder: &str) -> Result<()> {
        match &self.on_relaunch {
            Some(f) => f(folder),
            None => Err(anyhow::anyhow!("relaunch not expected")),
        }
    }
    fn cli_candidates(&self) -> Vec<String> {
        self.cli_candidates.clone()
    }
    fn link_dir(&self) -> String {
        self.link_dir.clone()
    }
}

fn obsidian(env: TestEnv) -> Obsidian {
    Obsidian::new(Arc::new(env))
}

/// Tests that spawn freshly written scripts run one at a time: a script still open for writing fails to exec (ETXTBSY).
static SCRIPTS: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SCRIPTS.lock().unwrap_or_else(|e| e.into_inner())
}

fn write_script(path: &Path, body: &str) {
    write_file(&path.to_string_lossy(), body.as_bytes(), 0o755).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn ensure_vault_registers_once() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("docs");
    std::fs::create_dir(&vault).unwrap();
    let vault = vault.to_string_lossy().to_string();
    let cfg = dir.path().join("obsidian.json");
    let seed =
        "{\n  \"theme\": \"moonstone\",\n  \"vaults\": {\n    \"abc\": {\"path\": \"/other\", \"ts\": 1}\n  }\n}\n";
    write_file(&cfg.to_string_lossy(), seed.as_bytes(), 0o600).unwrap();
    let obs = obsidian(TestEnv {
        config_file: cfg.to_string_lossy().to_string(),
        ..Default::default()
    });

    let note = obs.ensure_vault(&vault).unwrap();
    assert!(note.contains("Registered"), "note = {note:?}");
    let marker = std::fs::read(Path::new(&vault).join(".obsidian").join("app.json")).unwrap();
    assert_eq!(String::from_utf8_lossy(&marker), "{}\n");

    let again = obs.ensure_vault(&vault).unwrap();
    assert!(again.contains("already registered"), "second note = {again:?}");
    let raw = std::fs::read(&cfg).unwrap();
    let doc: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(doc["theme"].as_str(), Some("moonstone"));
    let vaults = doc["vaults"].as_object().unwrap();
    assert_eq!(vaults.len(), 2, "vaults = {vaults:?}");
    assert!(
        vaults.values().any(|v| v["path"].as_str() == Some(vault.as_str())),
        "vault path missing"
    );
    let mode = std::fs::metadata(&cfg).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "mode = {mode:o}");
}

#[test]
fn ensure_vault_reports_bad_config() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("docs");
    std::fs::create_dir(&vault).unwrap();
    let cfg = dir.path().join("obsidian.json");
    let obs = obsidian(TestEnv {
        config_file: cfg.to_string_lossy().to_string(),
        ..Default::default()
    });

    std::fs::write(&cfg, "[1]").unwrap();
    let err = obs.ensure_vault(&vault.to_string_lossy()).unwrap_err().to_string();
    assert_eq!(
        err,
        "obsidian config: json: cannot unmarshal array into Go value of type map[string]interface {}"
    );

    std::fs::write(&cfg, "{\"vaults\": 3}").unwrap();
    let err = obs.ensure_vault(&vault.to_string_lossy()).unwrap_err().to_string();
    assert_eq!(err, "obsidian config: vaults is not an object");

    let none = obsidian(TestEnv::default());
    let err = none.ensure_vault(&vault.to_string_lossy()).unwrap_err().to_string();
    assert_eq!(err, "Obsidian config path is not set");
    let err = none.ensure_vault("/definitely/missing/folder").unwrap_err().to_string();
    assert_eq!(err, "folder /definitely/missing/folder does not exist");
}

#[test]
fn install_obsidian_command_links_bundle() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("obsidian-cli");
    write_script(&bin, "#!/bin/sh\nexit 0\n");
    let link = dir.path().join("bin");
    let bin_s = bin.to_string_lossy().to_string();
    let obs = obsidian(TestEnv {
        cli_candidates: vec![bin_s.clone()],
        link_dir: link.to_string_lossy().to_string(),
        ..Default::default()
    });

    let got = obs.install_command().unwrap();
    assert_eq!(got, link.join("obsidian").to_string_lossy());
    let target = std::fs::read_link(&got).unwrap();
    assert_eq!(target.to_string_lossy(), bin_s);
    let again = obs.install_command().unwrap();
    assert_eq!(again, got, "second cli = {again}");
}

#[test]
fn install_obsidian_command_missing() {
    let obs = obsidian(TestEnv::default());
    assert!(
        obs.install_command().is_err(),
        "expected an error when Obsidian is not installed"
    );
    assert_eq!(obs.bundled_cli(), "");
}

#[test]
fn install_obsidian_command_without_link_dir_returns_bundle() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("obsidian-cli");
    write_script(&bin, "#!/bin/sh\nexit 0\n");
    let bin_s = bin.to_string_lossy().to_string();
    let obs = obsidian(TestEnv {
        cli_candidates: vec!["/missing/cli".into(), bin_s.clone()],
        ..Default::default()
    });
    assert_eq!(obs.bundled_cli(), bin_s);
    assert_eq!(obs.install_command().unwrap(), bin_s);
}

#[test]
fn obsidian_status_targets_folder() {
    let args = obsidian_status("/tmp/docs");
    assert_eq!(args, vec!["obsidian", "vault=docs", "vault", "info=path"]);
    assert_eq!(vault_name("/tmp/my docs/"), "my docs");
}

#[test]
fn open_vault_code_quotes_path() {
    let code = open_vault_code("/tmp/my docs");
    assert!(
        code.contains(r#"sendSync("vault-open","/tmp/my docs",false)"#),
        "code = {code}"
    );
}

#[test]
fn open_vault_uses_the_active_vault_when_the_name_is_shared() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("docs");
    std::fs::create_dir(&vault).unwrap();
    let vault = vault.to_string_lossy().to_string();
    let opened = dir.path().join("opened");
    let script = dir.path().join("obsidian");
    let body = format!(
        "#!/bin/sh\nopened=\"{}\"\nif [ \"$1\" = \"eval\" ]; then echo '=> true'; exit 0; fi\nif [ \"$1\" = \"{vault}\" ]; then touch \"$opened\"; echo \"{vault}\"; exit 0; fi\nif [ -f \"$opened\" ] && [ \"$1\" = \"vault\" ]; then echo \"{vault}\"; exit 0; fi\necho \"/other/docs\"\n",
        opened.display()
    );
    write_script(&script, &body);
    obsidian(TestEnv::default())
        .open_vault(&script.to_string_lossy(), &vault)
        .unwrap();
}

#[test]
fn open_vault_rejects_empty_cli() {
    let err = obsidian(TestEnv::default())
        .open_vault(" ", "/tmp")
        .unwrap_err()
        .to_string();
    assert_eq!(err, "Obsidian CLI is not set");
}

#[test]
fn open_vault_waits_until_cli_reports_path() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("docs");
    std::fs::create_dir(&vault).unwrap();
    let vault = vault.to_string_lossy().to_string();
    let script = dir.path().join("obsidian");
    let body = format!("#!/bin/sh\ncase \"$1\" in\n  eval) echo '=> true' ;;\n  *) echo \"{vault}\" ;;\nesac\n");
    write_script(&script, &body);
    obsidian(TestEnv::default())
        .open_vault(&script.to_string_lossy(), &vault)
        .unwrap();
}

#[test]
fn open_vault_relaunches_when_cli_cannot_see_vault() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("docs");
    std::fs::create_dir(&vault).unwrap();
    let vault = vault.to_string_lossy().to_string();
    let ready = dir.path().join("ready");
    let script = dir.path().join("obsidian");
    let body = format!(
        "#!/bin/sh\nif [ -f \"{}\" ]; then echo \"{vault}\"; else echo 'Vault not found.'; fi\n",
        ready.display()
    );
    write_script(&script, &body);
    let expected = vault.clone();
    let ready2 = ready.clone();
    let obs = obsidian(TestEnv {
        on_relaunch: Some(Box::new(move |folder: &str| {
            assert_eq!(folder, expected, "relaunch folder = {folder}");
            write_file(&ready2.to_string_lossy(), b"1", 0o644)?;
            Ok(())
        })),
        ..Default::default()
    });
    obs.open_vault(&script.to_string_lossy(), &vault).unwrap();
}

#[test]
fn same_path_ignores_lexical_differences() {
    assert!(same_path("/a/b/../c", "/a/c"));
    assert!(!same_path("/a/c", "/a/d"));
}
