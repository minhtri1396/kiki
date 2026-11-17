//! `ensure` and `index`: add the missing ignore rules to the `.gitignore` at the work tree root.

use crate::entries::Entry;
use crate::file::{append_patterns, write_file};
use crate::rules::{missing_patterns, HEADER_COMMENT, INDEX_HEADER};
use crate::worktree::{find_work_tree, want_for};
use anyhow::{anyhow, Result};
use kiki_util::path;
use std::io::ErrorKind;

/// Ensure adds ignore rules for entries when dir is inside a Git work tree.
/// The returned path is the .gitignore file that was written. It is empty when
/// nothing changed, including when dir is not inside a work tree.
/// Paths outside that work tree are left alone. Rules already present are kept.
pub fn ensure(dir: &str, entries: &[Entry]) -> Result<String> {
    ensure_with(dir, entries, HEADER_COMMENT)
}

/// Index ignores CodeGraph and Obsidian directories in the repository at dir.
/// names are ".codegraph" and ".obsidian". A folder that is not inside a Git work tree is left unchanged.
pub fn index(dir: &str, names: &[&str]) -> Result<String> {
    let dir = dir.trim();
    if dir.is_empty() || names.is_empty() {
        return Err(anyhow!("index path is invalid"));
    }
    let mut entries = Vec::with_capacity(names.len());
    for name in names {
        let name = name.trim().trim_matches(['/', '\\']);
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err(anyhow!("index path is invalid"));
        }
        entries.push(Entry::inside(dir, name));
    }
    ensure_with(dir, &entries, INDEX_HEADER)
}

fn ensure_with(dir: &str, entries: &[Entry], header: &str) -> Result<String> {
    let Some(root) = find_work_tree(dir)? else {
        return Ok(String::new());
    };
    let mut wants = Vec::with_capacity(entries.len());
    for entry in entries {
        wants.extend(want_for(&root, entry)?);
    }
    if wants.is_empty() {
        return Ok(String::new());
    }
    let file = path::join(&[&root, ".gitignore"]);
    let text = read_text(&file)?;
    let missing = missing_patterns(&text, &wants);
    if missing.is_empty() {
        return Ok(String::new());
    }
    write_file(&file, &append_patterns(&text, header, &missing))?;
    Ok(file)
}

/// The file's text; empty when it does not exist.
fn read_text(file: &str) -> Result<String> {
    match std::fs::read(file) {
        Ok(raw) => Ok(String::from_utf8_lossy(&raw).into_owned()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}
