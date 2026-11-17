//! Moves files an older KIKI wrote beside the app into the .kiki folder.

use std::fs;

use anyhow::{anyhow, Result};

use kiki_util::path::{clean, dir, join};

use super::{parse_doc, read_doc, write_doc};
use crate::fsutil;
use crate::store::{data_dir, path};

/// Migrate moves files an older KIKI wrote beside the app into the .kiki folder.
/// Repository folders stay where they are. Agent files always end up in .kiki/agents.
pub fn migrate(root: &str) -> Result<()> {
    let root = fsutil::abs(root.trim())?;
    let root = clean(&root);
    let dest_dir = data_dir(&root);
    match fs::metadata(join(&[&dest_dir, "kiki.yaml"])) {
        Ok(_) => {
            pin_agents(&root)?;
            return drop_models_files(&root);
        }
        Err(err) => {
            if !fsutil::not_exist(&err) {
                return Err(err.into());
            }
        }
    }
    let legacy_setup = join(&[&root, "kiki.yaml"]);
    match fs::metadata(&legacy_setup) {
        Ok(_) => {}
        Err(err) => {
            if fsutil::not_exist(&err) {
                return drop_models_files(&root);
            }
            return Err(err.into());
        }
    }
    let agent_path = legacy_agent_path(&legacy_setup)?;
    fsutil::mkdir_all(&dest_dir, 0o755)?;
    let old_agents = join(&[&root, "agents"]);
    let new_agents = join(&[&dest_dir, "agents"]);
    if agent_path.is_empty() || clean(&agent_path) == old_agents {
        move_path(&old_agents, &new_agents)?;
    }
    for name in ["kiki.db", "kiki.db-shm", "kiki.db-wal", "kiki.db-journal"] {
        move_path(&join(&[&root, name]), &join(&[&dest_dir, name]))?;
    }
    move_path(&legacy_setup, &join(&[&dest_dir, "kiki.yaml"]))?;
    pin_agents(&root)?;
    drop_models_files(&root)
}

/// dropModelsFiles removes the old per-agent model list. Tool and model live in each agent file.
fn drop_models_files(root: &str) -> Result<()> {
    let data = data_dir(root);
    for file in [
        join(&[root, "models.yaml"]),
        join(&[root, "models.yaml.tmp"]),
        join(&[&data, "models.yaml"]),
        join(&[&data, "models.yaml.tmp"]),
    ] {
        fsutil::remove_if_exists(&file)?;
    }
    Ok(())
}

fn legacy_agent_path(file: &str) -> Result<String> {
    let raw = fs::read(file)?;
    let doc = parse_doc(&String::from_utf8_lossy(&raw))?;
    Ok(doc.agent_path.trim().to_string())
}

fn pin_agents(root: &str) -> Result<()> {
    let setup = path(root);
    let agent_path = legacy_agent_path(&setup)?;
    let fixed = join(&[&data_dir(root), "agents"]);
    adopt_agents(&agent_path, &fixed)?;
    if agent_path.trim().is_empty() {
        return Ok(());
    }
    set_agent_path(&setup, "")
}

fn adopt_agents(from: &str, to: &str) -> Result<()> {
    // Cleaning an empty path gives ".", so an empty setting reaches the checks below as ".".
    let from = clean(from.trim());
    let to = clean(to);
    if from.is_empty() || from == to {
        return Ok(());
    }
    let info = match fs::metadata(&from) {
        Ok(info) => info,
        Err(err) => {
            if fsutil::not_exist(&err) {
                return Ok(());
            }
            return Err(err.into());
        }
    };
    if !info.is_dir() {
        return Ok(());
    }
    if dir_has_content(&to)? {
        return Ok(());
    }
    match fs::remove_dir_all(&to) {
        Ok(()) => {}
        Err(err) if fsutil::not_exist(&err) => {}
        Err(err) => {
            // A plain file in the way is removed too.
            if fs::symlink_metadata(&to).map(|m| !m.is_dir()).unwrap_or(false) {
                fs::remove_file(&to)?;
            } else {
                return Err(err.into());
            }
        }
    }
    fsutil::mkdir_all(&dir(&to), 0o755)?;
    fs::rename(&from, &to)?;
    Ok(())
}

/// Reports whether `dir` holds an entry that is not hidden. A missing folder is empty; a file counts as content.
fn dir_has_content(dir: &str) -> Result<bool> {
    let info = match fs::metadata(dir) {
        Ok(info) => info,
        Err(err) => {
            if fsutil::not_exist(&err) {
                return Ok(false);
            }
            return Err(err.into());
        }
    };
    if !info.is_dir() {
        return Ok(true);
    }
    // Hidden entries (and everything inside hidden folders) are skipped; the first visible entry counts.
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with('.') {
            return Ok(true);
        }
    }
    Ok(false)
}

fn set_agent_path(file: &str, agent_path: &str) -> Result<()> {
    let mut doc = read_doc(file)?;
    doc.agent_path = agent_path.to_string();
    write_doc(file, &doc)
}

fn move_path(from: &str, to: &str) -> Result<()> {
    match fs::metadata(from) {
        Ok(_) => {}
        Err(err) => {
            if fsutil::not_exist(&err) {
                return Ok(());
            }
            return Err(err.into());
        }
    }
    match fs::metadata(to) {
        Ok(_) => return Err(anyhow!("{to} already exists")),
        Err(err) => {
            if !fsutil::not_exist(&err) {
                return Err(err.into());
            }
        }
    }
    fs::rename(from, to)?;
    Ok(())
}
