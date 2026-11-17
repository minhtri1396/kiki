//! The team folder API: validate the folder, list and resolve member folders, compare and copy agent files.

use crate::fsops::{read_dir_names, real_dir};
use crate::paths::{overlaps, relative, safe_name};
use crate::tree::{install, read_tree};
use anyhow::{anyhow, Result};
use kiki_util::path;
use std::collections::HashMap;
use std::fs;

/// Checks that `raw` is an existing absolute directory.
pub fn dir(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(anyhow!("Choose a team folder first."));
    }
    if !path::is_abs(raw) {
        return Err(anyhow!("Enter the full path of the team folder."));
    }
    let dir = path::clean(raw);
    match fs::metadata(&dir) {
        Ok(info) if info.is_dir() => Ok(dir),
        _ => Err(anyhow!("That team folder is not available.")),
    }
}

/// Lists the member folders directly inside the team folder.
/// Hidden folders and symlinks are skipped.
pub fn members(raw: &str) -> Result<Vec<String>> {
    let dir = dir(raw)?;
    let entries = read_dir_names(&dir).map_err(|_| anyhow!("That team folder is not available."))?;
    let mut names: Vec<String> = Vec::new();
    for name in entries {
        if !safe_name(&name) || name.starts_with('.') {
            continue;
        }
        let Ok(info) = fs::symlink_metadata(path::join(&[&dir, &name])) else {
            continue;
        };
        if info.file_type().is_symlink() || !info.is_dir() {
            continue;
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// Reports whether `name` can be one member folder.
pub fn valid_name(name: &str) -> bool {
    let name = name.trim();
    safe_name(name) && !name.starts_with('.')
}

/// The child folder `name` of `dir`, checked to be a direct child.
fn direct_child(dir: &str, name: &str) -> Option<String> {
    let child = path::join(&[dir, name]);
    match relative(dir, &child) {
        Ok(r) if r == name => Some(child),
        _ => None,
    }
}

/// Returns the member folder for `name`, creating nothing.
/// The folder may be missing. An existing file or symlink is rejected.
pub fn user_dir(team: &str, name: &str) -> Result<String> {
    let dir = dir(team)?;
    let name = name.trim();
    if !valid_name(name) {
        return Err(anyhow!("Enter your name."));
    }
    let child = direct_child(&dir, name).ok_or_else(|| anyhow!("Enter your name."))?;
    match fs::symlink_metadata(&child) {
        Ok(info) => {
            if !info.is_dir() || info.file_type().is_symlink() {
                return Err(anyhow!("That name is not a folder in the team folder."));
            }
            Ok(child)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(child),
        Err(e) => Err(anyhow!("lstat {child}: {e}")),
    }
}

/// The agents folder inside a member folder.
pub fn agents_dir(member_dir: &str) -> String {
    path::join(&[&path::clean(member_dir), "agents"])
}

/// Reports whether `member_dir` contains a real agents directory.
pub fn has_agents(member_dir: &str) -> bool {
    real_dir(&agents_dir(member_dir))
}

/// The repositories folder inside a member folder.
pub fn repos_dir(member_dir: &str) -> String {
    path::join(&[&path::clean(member_dir), "repos"])
}

/// Reports whether `member_dir` contains a real repos directory.
pub fn has_repos(member_dir: &str) -> bool {
    real_dir(&repos_dir(member_dir))
}

/// Returns the directory for one member inside the team folder.
pub fn member(team: &str, name: &str) -> Result<String> {
    const NOT_IN_TEAM: &str = "That folder is not in the team folder.";
    let dir = dir(team)?;
    let name = name.trim();
    if !valid_name(name) {
        return Err(anyhow!(NOT_IN_TEAM));
    }
    let child = direct_child(&dir, name).ok_or_else(|| anyhow!(NOT_IN_TEAM))?;
    match fs::symlink_metadata(&child) {
        Ok(info) if info.is_dir() && !info.file_type().is_symlink() => Ok(child),
        _ => Err(anyhow!(NOT_IN_TEAM)),
    }
}

/// Reports whether the two folders contain the same agent files `copy` would read.
pub fn same(left: &str, right: &str) -> Result<bool> {
    let a = read_tree(left)?;
    let b = read_tree(right)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    let index: HashMap<&str, &Vec<u8>> = a.iter().map(|f| (f.rel.as_str(), &f.body)).collect();
    Ok(b.iter()
        .all(|file| matches!(index.get(file.rel.as_str()), Some(body) if **body == file.body)))
}

/// Replaces `agent_dir` with the files in `member_dir`.
/// `agent_dir` is left unchanged when the copy cannot be completed.
pub fn copy(member_dir: &str, agent_dir: &str) -> Result<()> {
    let member_dir = path::clean(member_dir.trim());
    let agent_dir = path::clean(agent_dir.trim());
    if member_dir.is_empty() || agent_dir.is_empty() || !path::is_abs(&member_dir) || !path::is_abs(&agent_dir) {
        return Err(anyhow!("The agent folder is not available."));
    }
    if overlaps(&member_dir, &agent_dir) {
        return Err(anyhow!("The team folder cannot be the local agents folder."));
    }
    let files = read_tree(&member_dir)?;
    if files.is_empty() {
        return Err(anyhow!("That member folder has no agent files to copy."));
    }
    install(&agent_dir, files)
}
