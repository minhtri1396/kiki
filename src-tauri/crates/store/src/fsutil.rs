//! Small file system helpers the store needs on top of `std::fs`: file modes, atomic writes, absolute paths.
//! Lexical path handling (clean, join, dir) lives in `kiki_util::path`.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use kiki_util::path::{clean, join};

/// `path` made absolute against the current directory and cleaned.
pub(crate) fn abs(path: &str) -> io::Result<String> {
    if kiki_util::path::is_abs(path) {
        return Ok(clean(path));
    }
    let cwd = std::env::current_dir()?;
    Ok(join(&[&cwd.to_string_lossy(), path]))
}

/// Creates `path` and its parents with the given mode.
pub(crate) fn mkdir_all(path: &str, mode: u32) -> io::Result<()> {
    fs::DirBuilder::new().recursive(true).mode(mode).create(path)
}

/// Writes `data` to `path`, replacing it. The mode applies when the file is created.
pub(crate) fn write_file(path: &str, data: &[u8], mode: u32) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)?;
    file.write_all(data)
}

/// Sets the permission bits of `path`.
pub(crate) fn chmod(path: &str, mode: u32) -> io::Result<()> {
    fs::set_permissions(Path::new(path), fs::Permissions::from_mode(mode))
}

/// True when the error says the path does not exist.
pub(crate) fn not_exist(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::NotFound
}

/// Removes a file; a missing file is success.
pub(crate) fn remove_if_exists(path: &str) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(err) if !not_exist(&err) => Err(err),
        _ => Ok(()),
    }
}

/// Writes `text` to `<path>.tmp` and renames it over `path`, so readers never see a half-written file.
pub(crate) fn write_atomic(path: &str, text: &[u8], mode: u32) -> io::Result<()> {
    let tmp = format!("{path}.tmp");
    write_file(&tmp, text, mode)?;
    fs::rename(&tmp, path)
}
