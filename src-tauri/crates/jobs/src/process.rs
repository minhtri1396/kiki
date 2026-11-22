//! Running a command with stdout and stderr on one pipe, killing it when the context ends.

use kiki_util::{Ctx, CtxErr};
use std::fmt;
use std::io::{ErrorKind, Read};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Why a command did not finish cleanly. The text is shown to users as the job error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandError(String);

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CommandError {}

/// The PATH the commands run with: the usual tool folders in front of the inherited PATH.
pub(crate) fn path_value() -> String {
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty()).unwrap_or_default();
    let extra = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        kiki_util::path::join(&[&home, ".npm-global", "bin"]),
        kiki_util::path::join(&[&home, ".local", "bin"]),
    ];
    let base = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = extra.to_vec();
    parts.push(base);
    parts.join(":")
}

/// The error text for a failed exit status.
fn exit_error(status: ExitStatus) -> CommandError {
    if let Some(sig) = status.signal() {
        let name = match sig {
            1 => "hangup".to_string(),
            2 => "interrupt".to_string(),
            3 => "quit".to_string(),
            6 => "abort trap".to_string(),
            9 => "killed".to_string(),
            13 => "broken pipe".to_string(),
            14 => "alarm clock".to_string(),
            15 => "terminated".to_string(),
            n => format!("signal {n}"),
        };
        return CommandError(format!("signal: {name}"));
    }
    CommandError(format!("exit status {}", status.code().unwrap_or(-1)))
}

/// The error text for a spawn failure (`fork/exec <bin>: <reason>` or `chdir <dir>: <reason>`).
fn spawn_error(bin: &str, dir: &str, e: &std::io::Error) -> CommandError {
    let mut msg = e.to_string();
    if let Some(i) = msg.find(" (os error") {
        msg.truncate(i);
    }
    let mut chars = msg.chars();
    let msg = match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => msg.clone(),
    };
    if e.kind() == ErrorKind::NotFound && !dir.is_empty() && !Path::new(dir).exists() {
        CommandError(format!("chdir {dir}: {msg}"))
    } else {
        CommandError(format!("fork/exec {bin}: {msg}"))
    }
}

/// Starts `bin args` in `dir` with the command PATH; returns the child and the read end of its combined output.
fn spawn(bin: &str, args: &[String], dir: &str) -> Result<(Child, std::io::PipeReader), CommandError> {
    let (reader, writer) = std::io::pipe().map_err(|e| CommandError(e.to_string()))?;
    let writer2 = writer.try_clone().map_err(|e| CommandError(e.to_string()))?;
    let mut cmd = Command::new(bin);
    cmd.args(args);
    if !dir.is_empty() {
        cmd.current_dir(dir);
    }
    cmd.env("PATH", path_value());
    cmd.stdin(Stdio::null()).stdout(writer).stderr(writer2);
    let child = cmd.spawn().map_err(|e| spawn_error(bin, dir, &e))?;
    // Drop the Command so the parent's copies of the pipe's write end close.
    drop(cmd);
    Ok((child, reader))
}

/// Feeds everything the child writes to `on_chunk` until the pipe closes.
fn pump(mut reader: std::io::PipeReader, mut on_chunk: impl FnMut(&[u8])) {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => on_chunk(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

/// Waits for the child. When `ctx` finishes first the child is killed and the context error is remembered.
/// `poll` runs each pass so output can be delivered while the child is still running.
fn wait_for_exit(child: &mut Child, ctx: &Ctx, mut poll: impl FnMut()) -> Result<(), CommandError> {
    let mut killed: Option<CtxErr> = None;
    let status = loop {
        poll();
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(e) => return Err(CommandError(e.to_string())),
        }
        if killed.is_none() {
            if let Some(e) = ctx.err() {
                let _ = child.kill();
                killed = Some(e);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    poll();
    if status.success() {
        return killed.map_or(Ok(()), |e| Err(CommandError(e.to_string())));
    }
    Err(exit_error(status))
}

/// Runs `bin args` in `dir`, sending stdout and stderr interleaved to `on_chunk` as they arrive.
/// The process is killed when `ctx` finishes.
pub(crate) fn run_combined(
    ctx: &Ctx,
    bin: &str,
    args: &[String],
    dir: &str,
    mut on_chunk: impl FnMut(&[u8]) + Send,
) -> Result<(), CommandError> {
    if let Some(e) = ctx.err() {
        return Err(CommandError(e.to_string()));
    }
    let (mut child, reader) = spawn(bin, args, dir)?;
    let (tx, rx) = mpsc::channel();
    let reading = std::thread::spawn(move || {
        pump(reader, |chunk| {
            let _ = tx.send(chunk.to_vec());
        });
    });
    let outcome = wait_for_exit(&mut child, ctx, || {
        while let Ok(chunk) = rx.try_recv() {
            on_chunk(&chunk);
        }
    });
    // A killed CLI can leave the pipe open (a child still holds the write end). Waiting on that
    // read pins the check, so the refresh spinner never stops. Keep what arrives, then move on.
    let grace = Instant::now() + Duration::from_millis(200);
    loop {
        let left = grace.saturating_duration_since(Instant::now());
        if left.is_zero() {
            while let Ok(chunk) = rx.try_recv() {
                on_chunk(&chunk);
            }
            break;
        }
        match rx.recv_timeout(left) {
            Ok(chunk) => on_chunk(&chunk),
            Err(RecvTimeoutError::Disconnected) | Err(RecvTimeoutError::Timeout) => break,
        }
    }
    if reading.is_finished() {
        let _ = reading.join();
    }
    outcome
}

/// The interleaved output of a command run under `ctx`, and its error text if it failed.
pub(crate) fn combined_output(ctx: &Ctx, bin: &str, args: &[String], dir: &str) -> (String, Option<String>) {
    let mut buf: Vec<u8> = Vec::new();
    let result = run_combined(ctx, bin, args, dir, |chunk| buf.extend_from_slice(chunk));
    (
        String::from_utf8_lossy(&buf).into_owned(),
        result.err().map(|e| e.to_string()),
    )
}
