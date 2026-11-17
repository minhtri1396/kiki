//! Pattern: Dependency injection. `CliHost` is the one place the console touches the machine
//! (is a CLI installed, start a CLI), so tests pass a fake instead of a process-wide hook.

use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use anyhow::{anyhow, Result};

use kiki_util::{look_path, Ctx, CtxErr};

use crate::text::{one_line, truncate_bytes};

/// Waits for a started CLI and reports how it ended.
pub type WaitFn = Box<dyn FnOnce() -> Result<()> + Send>;

/// One started CLI: its output streams and a way to wait for it.
pub struct CommandRun {
    pub stdout: Option<Box<dyn Read + Send>>,
    pub stderr: Option<Box<dyn Read + Send>>,
    pub wait: WaitFn,
}

/// What the console needs from the machine it runs on.
pub trait CliHost: Send + Sync {
    /// Reports whether a CLI is on PATH.
    fn command_exists(&self, name: &str) -> bool;

    /// Starts `bin` with `args` in `dir`. The CLI is killed when `ctx` finishes.
    fn start(&self, ctx: &Ctx, dir: &str, bin: &str, args: &[String]) -> Result<CommandRun>;
}

/// The real machine: PATH lookup and child processes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHost;

impl CliHost for SystemHost {
    fn command_exists(&self, name: &str) -> bool {
        look_path(name).is_some()
    }

    fn start(&self, ctx: &Ctx, dir: &str, bin: &str, args: &[String]) -> Result<CommandRun> {
        // A context that is already done never starts a process.
        if let Some(e) = ctx.err() {
            return Err(anyhow::Error::new(e));
        }
        let mut cmd = Command::new(bin);
        cmd.args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                let message = truncate_bytes(&one_line(&err.to_string()), 300);
                return Err(anyhow!("{}: {}", bin, message));
            }
        };
        let stdout = child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>);
        let stderr = child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>);
        let ctx = ctx.clone();
        // The watcher kills the process when the context finishes and reaps it.
        let watcher = std::thread::spawn(move || -> (std::io::Result<ExitStatus>, Option<CtxErr>) {
            let mut killed: Option<CtxErr> = None;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return (Ok(status), killed),
                    Err(e) => return (Err(e), killed),
                    Ok(None) => {}
                }
                if killed.is_none() {
                    if let Some(e) = ctx.err() {
                        let _ = child.kill();
                        killed = Some(e);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let wait: WaitFn = Box::new(move || {
            let (status, killed) = watcher.join().map_err(|_| anyhow!("waiting for the command failed"))?;
            if let Some(e) = killed {
                return Err(anyhow::Error::new(e));
            }
            exit_result(status?)
        });
        Ok(CommandRun { stdout, stderr, wait })
    }
}

/// Maps an exit status to `Ok` or the message shell users know ("exit status 3", "signal: 9").
fn exit_result(status: ExitStatus) -> Result<()> {
    use std::os::unix::process::ExitStatusExt;
    if status.success() {
        return Ok(());
    }
    match (status.code(), status.signal()) {
        (Some(code), _) => Err(anyhow!("exit status {}", code)),
        (None, Some(sig)) => Err(anyhow!("signal: {}", sig)),
        _ => Err(anyhow!("{}", status)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_streams_and_times_out() {
        let ctx = Ctx::background();
        let run = SystemHost
            .start(
                &ctx,
                "/tmp",
                "sh",
                &["-c".into(), "echo out; echo err >&2; exit 3".into()],
            )
            .unwrap();
        let mut out = String::new();
        run.stdout.unwrap().read_to_string(&mut out).unwrap();
        let mut err = String::new();
        run.stderr.unwrap().read_to_string(&mut err).unwrap();
        assert_eq!((out.as_str(), err.as_str()), ("out\n", "err\n"));
        assert_eq!((run.wait)().unwrap_err().to_string(), "exit status 3");

        let ctx = Ctx::background().with_timeout(Duration::from_millis(100));
        let run = SystemHost.start(&ctx, "/tmp", "sleep", &["5".into()]).unwrap();
        let err = (run.wait)().unwrap_err();
        assert_eq!(
            err.downcast_ref::<CtxErr>(),
            Some(&CtxErr::DeadlineExceeded),
            "err = {err}"
        );
    }

    #[test]
    fn a_finished_context_starts_nothing() {
        let ctx = Ctx::background().with_cancel();
        ctx.cancel();
        let err = SystemHost
            .start(&ctx, "/tmp", "sh", &[])
            .err()
            .expect("start must fail");
        assert_eq!(err.downcast_ref::<CtxErr>(), Some(&CtxErr::Canceled));
    }

    #[test]
    fn a_missing_program_names_itself() {
        let err = SystemHost
            .start(&Ctx::background(), "/tmp", "kiki-no-such-cli", &[])
            .err()
            .expect("start must fail");
        assert!(err.to_string().starts_with("kiki-no-such-cli: "), "err = {err}");
    }

    #[test]
    fn command_exists_uses_path() {
        assert!(SystemHost.command_exists("sh"));
        assert!(!SystemHost.command_exists("kiki-no-such-cli"));
    }
}
