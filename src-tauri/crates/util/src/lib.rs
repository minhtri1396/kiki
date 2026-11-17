//! Shared helpers used by every module: a cancellable context with a deadline
//! ([`Ctx`]) and a subprocess runner that honours it.

pub mod json;
pub mod path;

use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cancellation + deadline carrier. Cheap to clone; clones share the cancel flag.
///
/// Mirrors `context.Context`: `Ctx::background()`, `ctx.with_timeout(d)`,
/// `ctx.with_cancel()`, `ctx.done()`, `ctx.err()`.
#[derive(Clone, Debug)]
pub struct Ctx {
    deadline: Option<Instant>,
    cancelled: Arc<AtomicBool>,
    parent: Option<Box<Ctx>>,
}

/// Why a context finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtxErr {
    Canceled,
    DeadlineExceeded,
}

impl std::fmt::Display for CtxErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CtxErr::Canceled => f.write_str("context canceled"),
            CtxErr::DeadlineExceeded => f.write_str("context deadline exceeded"),
        }
    }
}

impl std::error::Error for CtxErr {}

impl Ctx {
    /// A context that never finishes.
    pub fn background() -> Ctx {
        Ctx {
            deadline: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: None,
        }
    }

    /// Child context that finishes after `d` or when the parent finishes.
    pub fn with_timeout(&self, d: Duration) -> Ctx {
        let mine = Instant::now() + d;
        let deadline = match self.deadline {
            Some(p) if p < mine => p,
            _ => mine,
        };
        Ctx {
            deadline: Some(deadline),
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: Some(Box::new(self.clone())),
        }
    }

    /// Child context with its own cancel flag (`ctx.cancel()`).
    pub fn with_cancel(&self) -> Ctx {
        Ctx {
            deadline: self.deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: Some(Box::new(self.clone())),
        }
    }

    /// Cancel this context (and every context derived from it).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Why the context finished, if it has.
    pub fn err(&self) -> Option<CtxErr> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Some(CtxErr::Canceled);
        }
        if let Some(p) = &self.parent {
            if let Some(e) = p.err() {
                return Some(e);
            }
        }
        match self.deadline {
            Some(d) if Instant::now() >= d => Some(CtxErr::DeadlineExceeded),
            _ => None,
        }
    }

    /// True once cancelled or past its deadline.
    pub fn done(&self) -> bool {
        self.err().is_some()
    }

    /// Time left before the deadline, if any.
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline.map(|d| d.saturating_duration_since(Instant::now()))
    }
}

impl Default for Ctx {
    fn default() -> Self {
        Ctx::background()
    }
}

/// Run `cmd` to completion, killing it when `ctx` finishes. stdout/stderr are captured.
/// Equivalent to `exec.CommandContext(ctx, ...).Output()` plus captured stderr.
/// On cancellation the error is `io::ErrorKind::TimedOut` whose message is the `CtxErr` text.
pub fn run_output(ctx: &Ctx, cmd: &mut Command) -> std::io::Result<Output> {
    run_with_input(ctx, cmd, None)
}

/// Like [`run_output`] but writes `input` to the child's stdin first.
pub fn run_with_input(ctx: &Ctx, cmd: &mut Command, input: Option<&[u8]>) -> std::io::Result<Output> {
    if let Some(e) = ctx.err() {
        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, e.to_string()));
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd.spawn()?;
    if let Some(data) = input {
        if let Some(mut stdin) = child.stdin.take() {
            let data = data.to_vec();
            std::thread::spawn(move || {
                let _ = stdin.write_all(&data);
            });
        }
    }
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut buf);
        }
        buf
    });
    let t_err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_end(&mut buf);
        }
        buf
    });
    let mut killed: Option<CtxErr> = None;
    let status: ExitStatus = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if killed.is_none() {
            if let Some(e) = ctx.err() {
                let _ = child.kill();
                killed = Some(e);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    if let Some(e) = killed {
        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, e.to_string()));
    }
    Ok(Output { status, stdout, stderr })
}

/// Look up `name` on PATH (like the shell does). Names containing a path separator are
/// checked directly.
pub fn look_path(name: &str) -> Option<std::path::PathBuf> {
    use std::path::{Path, PathBuf};
    fn executable(p: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(p) {
            Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    if name.contains('/') {
        let p = PathBuf::from(name);
        return if executable(&p) { Some(p) } else { None };
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Random lowercase hex string of `n` bytes (2n characters), like `hex(crypto/rand)`.
pub fn random_hex(n: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; n];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Current time as RFC 3339 in UTC (`time.Now().UTC().Format(time.RFC3339)`).
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
