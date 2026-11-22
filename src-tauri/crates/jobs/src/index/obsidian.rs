//! The Obsidian strategy: register the vault and open it, then watch `obsidian vault info=path` as the job and check.

use super::{IndexTool, Launch, Setup};
use crate::checks::{command_missing, is_dir, path_mentioned};
use crate::env::Env;
use crate::model::Check;
use crate::process::combined_output;
use crate::text::{clean_output, first_line, truncate_bytes};
use anyhow::{anyhow, Result};
use kiki_util::{path, Ctx};
use std::time::{Duration, Instant};

pub(super) struct Obsidian;

impl IndexTool for Obsidian {
    fn name(&self) -> &'static str {
        "obsidian"
    }

    fn dir_name(&self) -> &'static str {
        ".obsidian"
    }

    fn prepare(&self, _env: &Env, req: &Setup<'_>) -> Result<Launch> {
        let folder = req.folder.trim();
        let cli = kiki_toolchain::install_obsidian_command()?;
        if !is_dir(folder) {
            return Err(anyhow!("folder {folder} does not exist"));
        }
        let ignore_path = kiki_gitignore::index(folder, &[".codegraph", ".obsidian"])?;
        let mut note = kiki_toolchain::ensure_vault(folder)?;
        if !cli.is_empty() {
            note = format!("{note}\nObsidian CLI: {cli}");
        }
        if !ignore_path.is_empty() {
            note = format!("{note}\nGit ignore: {ignore_path}");
        }
        if let Err(err) = kiki_toolchain::open_vault(&cli, folder) {
            return Err(anyhow!("{note} ({err:#})"));
        }
        Ok(Launch {
            dir: folder.to_string(),
            args: kiki_toolchain::obsidian_check_args(folder),
            note,
        })
    }

    fn check(&self, env: &Env, ctx: &Ctx, _root: &str, folder: &str) -> Check {
        if folder.is_empty() {
            return Check::not_ready("folder is not set");
        }
        let abs = crate::checks::abs_path(folder).unwrap_or_else(|| folder.to_string());
        let exists = is_dir(&path::join(&[&abs, ".obsidian"]));
        let args = kiki_toolchain::obsidian_check_args(&abs);
        let bin = match (env.look_path)(&args[0]) {
            Ok(bin) => bin,
            Err(_) => return command_missing(exists, &args[0]),
        };
        let probe = Probe {
            bin,
            args,
            abs,
            exists,
            ctx: ctx.with_timeout(Duration::from_secs(30)),
            call_timeout: env.obsidian_call_timeout,
        };
        probe.check(env)
    }

    fn judge_success(&self, log: &str) -> Option<String> {
        log.contains("Vault not found.").then(|| "Vault not found.".to_string())
    }

    fn start_error(&self, launch: &Launch, err: anyhow::Error) -> anyhow::Error {
        anyhow!("{} ({err:#})", launch.note)
    }
}

/// The result of one `obsidian vault info=path` run.
struct Reading {
    text: String,
    run_err: Option<String>,
    ready: bool,
}

/// One Obsidian status check: the resolved CLI and the vault it asks about.
struct Probe {
    bin: String,
    args: Vec<String>,
    abs: String,
    exists: bool,
    ctx: Ctx,
    call_timeout: Duration,
}

impl Probe {
    fn run(&self) -> Reading {
        let named = self.reading(&self.args[1..]);
        if named.ready {
            return named;
        }
        let active = self.reading(&["vault".to_string(), "info=path".to_string()]);
        if active.ready {
            return active;
        }
        named
    }

    /// One status command. `ready` is true only when the output names this folder.
    /// The call has its own deadline, so a hung `vault=<name>` cannot skip the active-vault check.
    fn reading(&self, args: &[String]) -> Reading {
        let call = self.ctx.with_timeout(self.call_timeout);
        let (out, run_err) = combined_output(&call, &self.bin, args, &self.abs);
        let text = clean_output(&out);
        let ready = self.exists && run_err.is_none() && path_mentioned(&text, &self.abs);
        Reading { text, run_err, ready }
    }

    /// Runs the status command; waits and retries while the app is starting, and opens the vault once when
    /// no vault window is open.
    fn check(&self, env: &Env) -> Check {
        let mut reading = self.run();
        if self.app_is_starting(&reading) && sleep_context(&self.ctx, env.obsidian_retry_delay) {
            reading = self.run();
        }
        if self.no_vault_window(&reading) && kiki_toolchain::open_vault(&self.bin, &self.abs).is_ok() {
            reading = self.run();
        }
        self.summarize(reading)
    }

    /// The CLI answers `Command "vault" not found` while the app is still starting.
    fn app_is_starting(&self, reading: &Reading) -> bool {
        !reading.ready && reading.text.contains("Command \"vault\" not found") && self.ctx.err().is_none()
    }

    /// No vault window is open, so the vault has to be opened.
    fn no_vault_window(&self, reading: &Reading) -> bool {
        !reading.ready && reading.text.contains("Vault not found") && self.ctx.err().is_none()
    }

    fn summarize(&self, reading: Reading) -> Check {
        let Reading { text, run_err, ready } = reading;
        let mut summary = first_line(&text);
        if summary.is_empty() {
            if let Some(err) = run_err {
                summary = err;
            } else if !self.exists {
                summary = "Vault folder is missing".to_string();
            } else if !ready {
                summary = "Obsidian did not report this document folder".to_string();
            } else {
                summary = "Ready".to_string();
            }
        }
        Check {
            ready,
            dir_exists: self.exists,
            summary: truncate_bytes(summary, 400),
            checked_at: kiki_util::now_rfc3339(),
        }
    }
}

/// Waits `delay`. False when `ctx` finished first.
fn sleep_context(ctx: &Ctx, delay: Duration) -> bool {
    let end = Instant::now() + delay;
    loop {
        if ctx.err().is_some() {
            return false;
        }
        let now = Instant::now();
        if now >= end {
            return true;
        }
        std::thread::sleep((end - now).min(Duration::from_millis(10)));
    }
}
