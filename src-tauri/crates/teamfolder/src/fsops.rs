//! Small filesystem helpers.

use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

/// Creates `path` and its parents with mode 0755 (subject to umask).
pub(crate) fn mkdir_all(path: &str) -> std::io::Result<()> {
    fs::DirBuilder::new().recursive(true).mode(0o755).create(path)
}

/// Removes a file or a directory tree; a missing path is not an error. Symlinks are removed, not followed.
pub(crate) fn remove_all(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Names of the entries of `dir`, sorted by name.
pub(crate) fn read_dir_names(dir: &str) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// Reports whether `path` is a directory that is not a symlink.
pub(crate) fn real_dir(path: &str) -> bool {
    match fs::symlink_metadata(path) {
        Ok(info) => info.is_dir() && !info.file_type().is_symlink(),
        Err(_) => false,
    }
}
