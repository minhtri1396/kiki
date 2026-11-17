//! File system helpers for the agent folder: errors worded as `op path: reason`, atomic writes, moves and
//! the directory walks that find agent markdown files.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{anyhow, Result};
use kiki_util::path::{clean, dir, join};

/// The wording of an OS error without the code: the errno text in lower case, like `no such file or directory`.
fn os_msg(e: &io::Error) -> String {
    let mut msg = e.to_string();
    if let Some(i) = msg.find(" (os error") {
        msg.truncate(i);
    }
    let mut chars = msg.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => msg,
    }
}

/// An error that names the operation and the path: `op path: reason`.
pub(crate) fn path_err(op: &str, path: &str, e: &io::Error) -> anyhow::Error {
    anyhow!("{op} {path}: {}", os_msg(e))
}

pub(crate) fn not_exist(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound
}

/// Creates `path` and its parents with mode 0755.
pub(crate) fn mkdir_all(path: &str) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(path)
        .map_err(|e| path_err("mkdir", path, &e))
}

/// Renames `src` to `dst`, copying and removing when a rename is not possible (another device).
pub(crate) fn move_file(src: &str, dst: &str) -> Result<()> {
    if clean(src) == clean(dst) {
        return Ok(());
    }
    mkdir_all(&dir(dst))?;
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let mut input = fs::File::open(src).map_err(|e| path_err("open", src, &e))?;
    let info = input.metadata().map_err(|e| path_err("stat", src, &e))?;
    let tmp = format!("{dst}.tmp");
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(info.permissions().mode() & 0o777)
        .open(&tmp)
        .map_err(|e| path_err("open", &tmp, &e))?;
    if let Err(e) = io::copy(&mut input, &mut out) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!("{}", os_msg(&e)));
    }
    drop(out);
    if let Err(e) = fs::rename(&tmp, dst) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!("rename {tmp} {dst}: {}", os_msg(&e)));
    }
    fs::remove_file(src).map_err(|e| path_err("remove", src, &e))
}

/// Writes `text` to `path` through a `.tmp` file and a rename, creating the parent folder.
pub(crate) fn write_atomic(path: &str, text: &str) -> Result<()> {
    mkdir_all(&dir(path))?;
    let tmp = format!("{path}.tmp");
    {
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&tmp)
            .map_err(|e| path_err("open", &tmp, &e))?;
        out.write_all(text.as_bytes())
            .map_err(|e| path_err("write", &tmp, &e))?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!("rename {tmp} {path}: {}", os_msg(&e)));
    }
    Ok(())
}

/// True when the name ends in `.md`, ignoring case.
fn has_md_ext(name: &str) -> bool {
    match name.rfind('.') {
        Some(i) => name[i..].eq_ignore_ascii_case(".md"),
        None => false,
    }
}

/// Lists the entries of `dir`; a missing folder is an empty list.
fn read_dir_or_empty(dir: &str) -> Result<Vec<fs::DirEntry>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if not_exist(&e) => return Ok(Vec::new()),
        Err(e) => return Err(path_err("open", dir, &e)),
    };
    entries
        .map(|entry| entry.map_err(|e| path_err("readdirent", dir, &e)))
        .collect()
}

fn is_dir(entry: &fs::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
}

/// The `.md` files inside one category folder, as file names.
fn markdown_in(folder: &str) -> Result<Vec<String>> {
    let children = fs::read_dir(folder).map_err(|e| path_err("open", folder, &e))?;
    let mut names = Vec::new();
    for child in children {
        let child = child.map_err(|e| path_err("readdirent", folder, &e))?;
        let name = child.file_name().to_string_lossy().into_owned();
        if is_dir(&child) || name.starts_with('.') || !has_md_ext(&name) {
            continue;
        }
        names.push(name);
    }
    Ok(names)
}

/// The markdown files inside category folders as `(path, category)`, sorted by path.
pub(crate) fn category_files(root: &str) -> Result<Vec<(String, String)>> {
    let mut paths: Vec<(String, String)> = Vec::new();
    for entry in read_dir_or_empty(root)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_dir(&entry) || name.starts_with('.') {
            continue;
        }
        for child in markdown_in(&join(&[root, &name]))? {
            paths.push((join(&[root, &name, &child]), name.clone()));
        }
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(paths)
}

/// True when the agent folder holds any markdown file, at its top level or inside a category folder.
/// Every category folder is read, so an unreadable one is an error even when another has markdown.
pub(crate) fn has_agent_markdown(root: &str) -> Result<bool> {
    let mut found = false;
    for entry in read_dir_or_empty(root)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if is_dir(&entry) {
            found |= !markdown_in(&join(&[root, &name]))?.is_empty();
        } else {
            found |= has_md_ext(&name);
        }
    }
    Ok(found)
}

/// Removes `folder` when it is empty and inside `root` (never `root` itself).
pub(crate) fn remove_empty(folder: &str, root: &str) {
    let folder = clean(folder);
    let root = clean(root);
    if folder == root {
        return;
    }
    let inside = match Path::new(&folder).strip_prefix(&root) {
        Ok(rel) => !rel.as_os_str().is_empty(),
        Err(_) => false,
    };
    if !inside {
        return;
    }
    match fs::read_dir(&folder) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return;
            }
        }
        Err(_) => return,
    }
    let _ = fs::remove_dir(&folder);
}
