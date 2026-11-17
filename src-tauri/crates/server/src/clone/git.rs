//! The `git` program: cloning a repository (whole or one folder of it) and reading where a folder came from.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, Result};
use kiki_util::{run_output, Ctx};

use super::folder::{mkdir_all, take_clone, ScratchFolder};

/// The longest `git` output kept in an error message.
const MAX_ERROR_BYTES: usize = 2000;

/// Clones `remote` into `folder`. The clone is made in a scratch folder first and moved in only when it
/// worked, so a failed clone leaves `folder` as it was. A `sparse` folder checks out only that folder.
pub(crate) fn git_clone(ctx: &Ctx, remote: &str, folder: &str, sparse: &str) -> Result<()> {
    mkdir_all(Path::new(folder))?;
    let parent = Path::new(folder)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let scratch = ScratchFolder::create(parent, ".kiki-clone-")?;
    let target = scratch.path().to_string_lossy().into_owned();
    run_git_clone(ctx, remote, &target, sparse)?;
    if !sparse.is_empty() {
        sparse_checkout(ctx, &target, sparse)?;
    }
    take_clone(scratch.path(), Path::new(folder))
}

fn run_git_clone(ctx: &Ctx, remote: &str, folder: &str, sparse: &str) -> Result<()> {
    if sparse.is_empty() {
        return exec_git(ctx, &["clone", "--", remote, folder]);
    }
    let err = match exec_git(ctx, &["clone", "--filter=blob:none", "--sparse", "--", remote, folder]) {
        Ok(()) => return Ok(()),
        Err(err) => err,
    };
    if ctx.err().is_some() || !filter_rejected(&err) {
        return Err(err);
    }
    // The server cannot filter blobs: start over with a full sparse clone.
    fs::remove_dir_all(folder)?;
    mkdir_all(Path::new(folder))?;
    exec_git(ctx, &["clone", "--sparse", "--", remote, folder])
}

fn sparse_checkout(ctx: &Ctx, folder: &str, sparse: &str) -> Result<()> {
    exec_git(ctx, &["-C", folder, "sparse-checkout", "set", "--cone", "--", sparse])?;
    let is_dir = fs::metadata(Path::new(folder).join(sparse))
        .map(|m| m.is_dir())
        .unwrap_or(false);
    if !is_dir {
        return Err(anyhow!("folder {sparse} is not in this repository"));
    }
    Ok(())
}

fn filter_rejected(err: &anyhow::Error) -> bool {
    format!("{err:#}").to_lowercase().contains("filter")
}

/// Runs `git` and turns a failure into `git <command> failed: <output>` (or `timed out`).
fn exec_git(ctx: &Ctx, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(args).env("GIT_TERMINAL_PROMPT", "0");
    let (success, output, spawn_err) = match run_output(ctx, &mut cmd) {
        Ok(out) => {
            let mut text = out.stdout;
            text.extend_from_slice(&out.stderr);
            (out.status.success(), text, None)
        }
        Err(err) => (false, Vec::new(), Some(err)),
    };
    if success {
        return Ok(());
    }
    let name = git_command_name(args);
    if ctx.err().is_some() {
        return Err(anyhow!("git {name} timed out"));
    }
    let mut text = String::from_utf8_lossy(&output).trim().to_string();
    if text.is_empty() {
        text = match spawn_err {
            Some(err) => err.to_string(),
            None => "exit status 1".to_string(),
        };
    }
    if text.len() > MAX_ERROR_BYTES {
        // Cut at a byte count, which may split a character; the lossy decoding then shows it as `�`.
        text = format!("{}…", String::from_utf8_lossy(&text.as_bytes()[..MAX_ERROR_BYTES]));
    }
    Err(anyhow!("git {name} failed: {text}"))
}

/// The first word of the arguments that is not an option: the subcommand.
fn git_command_name<'a>(args: &[&'a str]) -> &'a str {
    args.iter()
        .find(|arg| !arg.is_empty() && !arg.starts_with('-'))
        .copied()
        .unwrap_or("command")
}

/// The clone URL recorded as `origin`. Empty means the folder was not cloned from Git.
pub(crate) fn git_origin(folder: &str) -> String {
    let folder = folder.trim();
    if folder.is_empty() || fs::metadata(Path::new(folder).join(".git")).is_err() {
        return String::new();
    }
    let ctx = Ctx::background().with_timeout(Duration::from_secs(2));
    let mut cmd = Command::new("git");
    cmd.args(["-C", folder, "remote", "get-url", "origin"])
        .env("GIT_TERMINAL_PROMPT", "0");
    match run_output(&ctx, &mut cmd) {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    }
}
