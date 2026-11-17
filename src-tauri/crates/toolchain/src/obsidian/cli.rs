//! Running the Obsidian CLI and reading its output.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CLI_TIMEOUT: Duration = Duration::from_secs(15);

/// Runs the CLI with a 15 second timeout. Returns its combined stdout+stderr,
/// trimmed, and whether it started and exited successfully.
pub(super) fn run_cli(cli: &str, args: &[&str]) -> (String, bool) {
    let Ok((mut reader, writer)) = std::io::pipe() else {
        return (String::new(), false);
    };
    let Ok(writer2) = writer.try_clone() else {
        return (String::new(), false);
    };
    let mut cmd = Command::new(cli);
    cmd.args(args).stdin(Stdio::null()).stdout(writer).stderr(writer2);
    let Ok(mut child) = cmd.spawn() else {
        return (String::new(), false);
    };
    // `cmd` still owns the parent's copies of the write ends; drop them so the reader sees EOF.
    drop(cmd);
    let collector = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        buf
    });
    let ok = wait_or_kill(&mut child, CLI_TIMEOUT);
    let out = collector.join().unwrap_or_default();
    (String::from_utf8_lossy(&out).trim().to_string(), ok)
}

/// Waits for the child; kills it after `timeout`. True only for a successful exit.
fn wait_or_kill(child: &mut std::process::Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {}
            Err(_) => return false,
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// `next` when it has text, otherwise `current`: the latest informative CLI output.
pub(super) fn prefer_text(current: &str, next: &str) -> String {
    let next = next.trim();
    if next.is_empty() {
        return current.to_string();
    }
    next.to_string()
}

/// The first non-empty trimmed line.
pub(super) fn first_output_line(text: &str) -> String {
    text.split('\n')
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Reports whether `eval` printed a true result.
pub(super) fn eval_true(text: &str) -> bool {
    text.split('\n').any(|line| matches!(line.trim(), "=> true" | "true"))
}
