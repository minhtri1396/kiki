//! The Orca command-line client: runs `orca ...` and checks that the app is running.

use crate::wire::{json_ok, parse_result};
use crate::Client;
use anyhow::{anyhow, Result};
use kiki_util::{run_output, Ctx};
use serde::Deserialize;
use std::process::Command;
use std::time::Duration;

/// How long one CLI call may take when the caller set no deadline.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(90);

/// The text of a failed exit status.
pub(crate) fn exit_text(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.code() {
        Some(code) => format!("exit status {code}"),
        None => match status.signal() {
            Some(9) => "signal: killed".to_string(),
            Some(sig) => format!("signal: {sig}"),
            None => "exit status unknown".to_string(),
        },
    }
}

/// Command output cut to 4000 bytes, for error messages.
pub(crate) fn trim(out: &[u8]) -> String {
    let text = String::from_utf8_lossy(out);
    let text = text.trim();
    if text.len() > 4000 {
        return format!("{}…", String::from_utf8_lossy(&text.as_bytes()[..4000]));
    }
    text.to_string()
}

fn json_valid(raw: &[u8]) -> bool {
    serde_json::from_slice::<serde::de::IgnoredAny>(raw).is_ok()
}

/// Runs the Orca CLI with a fixed binary, working directory and extra environment.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cli {
    bin: String,
    dir: String,
    env: Vec<String>,
}

impl From<&Client> for Cli {
    fn from(client: &Client) -> Cli {
        Cli {
            bin: client.bin.clone(),
            dir: client.dir.clone(),
            env: client.env.clone(),
        }
    }
}

impl Cli {
    /// Whether a CLI binary was named explicitly (otherwise the local Orca app is used where it can be).
    pub(crate) fn has_binary(&self) -> bool {
        !self.bin.is_empty()
    }

    /// Runs a command and requires an `"ok": true` reply.
    pub(crate) fn run<S: AsRef<str>>(&self, ctx: &Ctx, args: &[S]) -> Result<Vec<u8>> {
        let out = self.command_output(ctx, args)?;
        if !json_ok(&out) {
            return Err(anyhow!("orca {} failed\n{}", args[0].as_ref(), trim(&out)));
        }
        Ok(out)
    }

    /// Runs a command and returns stdout followed by stderr. A failing exit is only an error
    /// when the output is not JSON, because the CLI reports failures in a JSON reply.
    pub(crate) fn command_output<S: AsRef<str>>(&self, ctx: &Ctx, args: &[S]) -> Result<Vec<u8>> {
        let bounded;
        let ctx = if ctx.remaining().is_none() {
            bounded = ctx.with_timeout(DEFAULT_TIMEOUT);
            &bounded
        } else {
            ctx
        };
        let mut cmd = self.command(args);
        let (out, run_err) = match run_output(ctx, &mut cmd) {
            Ok(o) => {
                let mut combined = o.stdout;
                combined.extend_from_slice(&o.stderr);
                let err = if o.status.success() {
                    None
                } else {
                    Some(exit_text(&o.status))
                };
                (combined, err)
            }
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        if let Some(err) = run_err {
            if !json_valid(out.trim_ascii()) {
                return Err(anyhow!("orca {}: {}\n{}", args[0].as_ref(), err, trim(&out)));
            }
        }
        Ok(out)
    }

    fn command<S: AsRef<str>>(&self, args: &[S]) -> Command {
        let bin = if self.bin.is_empty() { "orca" } else { self.bin.as_str() };
        let mut cmd = Command::new(bin);
        cmd.args(args.iter().map(|a| a.as_ref()));
        if !self.dir.is_empty() {
            cmd.current_dir(&self.dir);
        }
        for entry in &self.env {
            match entry.split_once('=') {
                Some((k, v)) => cmd.env(k, v),
                None => cmd.env(entry, ""),
            };
        }
        cmd
    }

    /// Makes sure the Orca app is running, asking it to open when it is not.
    pub(crate) fn ensure_running(&self, ctx: &Ctx) -> Result<()> {
        let first = self.app_running(ctx);
        if matches!(first, Ok(true)) {
            return Ok(());
        }
        if let Err(open_err) = self.run(ctx, &["open", "--json"]) {
            return Err(match first {
                Err(err) => err,
                Ok(_) => open_err,
            });
        }
        if !self.app_running(ctx)? {
            return Err(anyhow!("Orca is not running. Open the Orca app and try again"));
        }
        Ok(())
    }

    fn app_running(&self, ctx: &Ctx) -> Result<bool> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct App {
            #[serde(deserialize_with = "crate::wire::nd")]
            running: bool,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Status {
            #[serde(deserialize_with = "crate::wire::nd")]
            app: App,
        }
        let out = self.run(ctx, &["status", "--json"])?;
        Ok(parse_result::<Status>(&out, "read Orca status")?.app.running)
    }
}
