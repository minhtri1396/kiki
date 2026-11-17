//! Opening a vault: ask the CLI, open the folder, relaunch Obsidian once, and poll until the CLI reports the path.

use super::cli::{eval_true, first_output_line, prefer_text, run_cli};
use super::env::ObsidianEnv;
use super::fsutil::{abs_path, same_path};
use super::vault_name;
use anyhow::{anyhow, Result};
use std::time::{Duration, Instant};

const OPEN_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(700);

pub(super) fn open_vault(env: &dyn ObsidianEnv, cli: &str, folder: &str) -> Result<()> {
    let folder = abs_path(folder)?;
    if cli.trim().is_empty() {
        return Err(anyhow!("Obsidian CLI is not set"));
    }
    let mut attempt = Attempt {
        env,
        cli,
        folder: &folder,
        last: String::new(),
        relaunched: false,
    };
    if vault_ready(cli, &folder).0 {
        return Ok(());
    }
    let deadline = Instant::now() + OPEN_TIMEOUT;
    loop {
        if attempt.poll() {
            return Ok(());
        }
        if Instant::now() > deadline {
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Err(attempt.failure())
}

/// One open-vault call: the target and the latest informative CLI output.
struct Attempt<'a> {
    env: &'a dyn ObsidianEnv,
    cli: &'a str,
    folder: &'a str,
    last: String,
    relaunched: bool,
}

impl Attempt<'_> {
    /// One pass of eval, open and (once) relaunch. True when the CLI reports the vault.
    fn poll(&mut self) -> bool {
        self.eval_open() || self.open_folder() || self.relaunch_if_missing()
    }

    fn note(&mut self, text: &str) {
        self.last = prefer_text(&self.last, text);
    }

    /// Checks readiness; on failure remembers the status text.
    fn confirm(&mut self) -> bool {
        let (ok, status) = vault_ready(self.cli, self.folder);
        if !ok {
            self.note(&status);
        }
        ok
    }

    /// Asks the running Obsidian to open the vault through its IPC channel.
    fn eval_open(&mut self) -> bool {
        let code = open_vault_code(self.folder);
        let (out, eval_ok) = run_cli(self.cli, &["eval", &format!("code={code}")]);
        self.note(&out);
        eval_ok && eval_true(&out) && self.confirm()
    }

    /// Passes the folder to the CLI, which opens it.
    fn open_folder(&mut self) -> bool {
        let (opened, open_ok) = run_cli(self.cli, &[self.folder]);
        if !open_ok {
            return false;
        }
        self.note(&opened);
        self.confirm()
    }

    /// When no vault window is open, relaunches Obsidian on the folder (once).
    fn relaunch_if_missing(&mut self) -> bool {
        if self.relaunched || !self.last.contains("Vault not found") {
            return false;
        }
        self.relaunched = true;
        if let Err(err) = self.env.relaunch(self.folder) {
            self.note(&err.to_string());
        }
        self.confirm()
    }

    fn failure(&self) -> anyhow::Error {
        let last = self.last.trim();
        if last.is_empty() {
            return anyhow!("Obsidian did not open {}", self.folder);
        }
        anyhow!("{}", first_output_line(last))
    }
}

fn vault_ready(cli: &str, folder: &str) -> (bool, String) {
    let (status, ok) = run_cli(cli, &[&format!("vault={}", vault_name(folder)), "vault", "info=path"]);
    if ok && status_matches(&status, folder) {
        return (true, status);
    }
    (false, status)
}

fn status_matches(output: &str, folder: &str) -> bool {
    output.split('\n').any(|line| {
        let line = line.trim();
        !line.is_empty() && same_path(line, folder)
    })
}

pub(super) fn open_vault_code(folder: &str) -> String {
    let path = kiki_util::json::marshal(&serde_json::Value::String(folder.to_string()));
    format!("window.electron.ipcRenderer.sendSync(\"vault-open\",{path},false)")
}
