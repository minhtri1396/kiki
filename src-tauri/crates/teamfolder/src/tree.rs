//! Reading the agent files of a member folder and installing them into the local agents folder.

use crate::fsops::{mkdir_all, read_dir_names, remove_all};
use crate::paths::{inside, safe_rel};
use anyhow::{anyhow, Result};
use kiki_util::path;
use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) const MAX_FILE: u64 = 8 << 20;
const MAX_TOTAL: i64 = 32 << 20;
const MAX_DEPTH: usize = 10;

/// One agent file: its path relative to the folder and its content.
pub(crate) struct FileBytes {
    pub(crate) rel: String,
    pub(crate) body: Vec<u8>,
}

/// Reads every agent file below `dir`, in name order. Hidden entries and symlinks are skipped.
pub(crate) fn read_tree(dir: &str) -> Result<Vec<FileBytes>> {
    match fs::metadata(dir) {
        Ok(info) if info.is_dir() => {}
        _ => return Err(anyhow!("That member folder is not available.")),
    }
    let mut files: Vec<FileBytes> = Vec::new();
    let mut total: i64 = 0;
    // A root symlink is not walked into.
    let root_info = fs::symlink_metadata(dir)?;
    if root_info.file_type().is_symlink() || !root_info.is_dir() {
        return Ok(files);
    }
    walk(dir, "", &mut files, &mut total)?;
    Ok(files)
}

/// Visits the entries of `abs` (relative path `rel_prefix` below the root) in name order.
fn walk(abs: &str, rel_prefix: &str, files: &mut Vec<FileBytes>, total: &mut i64) -> Result<()> {
    for name in read_dir_names(abs)? {
        let current = path::join(&[abs, &name]);
        let rel_path = if rel_prefix.is_empty() {
            name.clone()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let info = fs::symlink_metadata(&current)?;
        // Hidden directories are skipped whole, hidden files are ignored.
        if name.starts_with('.') || info.file_type().is_symlink() {
            continue;
        }
        if info.is_dir() {
            walk(&current, &rel_path, files, total)?;
        } else if info.file_type().is_file() {
            files.push(read_agent_file(&current, &name, &rel_path, info.len(), total)?);
        }
    }
    Ok(())
}

/// Reads one file after checking the size and depth limits; adds its size to the running total.
fn read_agent_file(current: &str, name: &str, rel_path: &str, len: u64, total: &mut i64) -> Result<FileBytes> {
    if len > MAX_FILE {
        return Err(anyhow!("agent file {name} is larger than 8MB"));
    }
    if rel_path.matches('/').count() > MAX_DEPTH {
        return Err(anyhow!("That member folder is too deep."));
    }
    let rel = safe_rel(rel_path)?;
    let body = fs::read(current)?;
    *total += body.len() as i64;
    if *total > MAX_TOTAL {
        return Err(anyhow!("The agent folder is larger than 32MB."));
    }
    Ok(FileBytes { rel, body })
}

/// Replaces the content of `dir` with `files`. The files are written to a temporary folder next to `dir`
/// first, so `dir` is untouched when writing fails.
pub(crate) fn install(dir: &str, files: Vec<FileBytes>) -> Result<()> {
    if dir.is_empty() || !path::is_abs(dir) {
        return Err(anyhow!("The agent folder is not available."));
    }
    let mut planned: Vec<FileBytes> = Vec::with_capacity(files.len());
    for file in files {
        planned.push(FileBytes {
            rel: safe_rel(&file.rel)?,
            body: file.body,
        });
    }
    ensure_dir(dir)?;
    // Removed when dropped, so a failed install leaves nothing behind.
    let tmp_guard = tempfile::Builder::new()
        .prefix(".kiki-agents-")
        .tempdir_in(path::dir(dir))?;
    let tmp = tmp_guard.path().to_string_lossy().into_owned();
    for file in &planned {
        write_planned(&tmp, file)?;
    }
    for name in read_dir_names(dir)? {
        remove_all(Path::new(&path::join(&[dir, &name])))?;
    }
    for name in read_dir_names(&tmp)? {
        fs::rename(path::join(&[&tmp, &name]), path::join(&[dir, &name]))?;
    }
    Ok(())
}

/// The folder must exist as a directory; a missing one is created.
fn ensure_dir(dir: &str) -> Result<()> {
    match fs::metadata(dir) {
        Ok(info) if info.is_dir() => Ok(()),
        Ok(_) => Err(anyhow!("The agent folder is not available.")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(mkdir_all(dir)?),
        Err(e) => Err(e.into()),
    }
}

fn write_planned(tmp: &str, file: &FileBytes) -> Result<()> {
    use std::io::Write;
    let dest = path::join(&[tmp, &file.rel]);
    if !inside(tmp, &dest) {
        return Err(anyhow!("agent file path {:?} is not valid", file.rel));
    }
    mkdir_all(&path::dir(&dest))?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&dest)?;
    f.write_all(&file.body)?;
    Ok(())
}
