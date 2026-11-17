//! The folder side of a clone: what counts as empty, the scratch folder, and moving the finished clone in.

use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Creates `path` and its parents (mode 0755). An existing folder is fine.
pub(crate) fn mkdir_all(path: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new().recursive(true).mode(0o755).create(path)
}

/// Index setup that may sit in a folder before the first clone: it is replaced by the clone.
fn is_setup_entry(name: &str) -> bool {
    matches!(name, ".DS_Store" | ".codegraph" | ".obsidian")
}

/// Whether `path` has no repository content. A missing folder is empty; `.DS_Store` and a CodeGraph or
/// Obsidian setup do not count.
pub(crate) fn folder_empty(path: &str) -> Result<bool> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(err) => return Err(err.into()),
    };
    for entry in entries {
        if !is_setup_entry(&entry?.file_name().to_string_lossy()) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A private folder next to the destination where the clone is made; it is removed when dropped.
pub(crate) struct ScratchFolder(PathBuf);

impl ScratchFolder {
    /// A new folder `dir/<prefix><random>` that only the current user can enter.
    pub(crate) fn create(dir: &Path, prefix: &str) -> std::io::Result<ScratchFolder> {
        loop {
            let path = dir.join(format!("{prefix}{}", kiki_util::random_hex(4)));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(ScratchFolder(path)),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Moves a finished clone into `dest` and drops the index setup that was there.
pub(crate) fn take_clone(src: &Path, dest: &Path) -> Result<()> {
    for entry in fs::read_dir(dest)? {
        let entry = entry?;
        if is_setup_entry(&entry.file_name().to_string_lossy()) {
            remove_all(&dest.join(entry.file_name()))?;
        }
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        fs::rename(src.join(entry.file_name()), dest.join(entry.file_name()))?;
    }
    Ok(())
}

/// Removes a file or folder; a path that is already gone is not an error.
fn remove_all(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}
