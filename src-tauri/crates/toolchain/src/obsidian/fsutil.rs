//! Small filesystem helpers shared by the Obsidian modules.

use anyhow::Result;
use kiki_util::path;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// Makes `p` absolute against the current directory and cleans it.
pub(super) fn abs_path(p: &str) -> Result<String> {
    if path::is_abs(p) {
        return Ok(path::clean(p));
    }
    let cwd = std::env::current_dir()?;
    Ok(path::join(&[&cwd.to_string_lossy(), p]))
}

/// Creates `p` and its parents with mode 0755 (subject to umask).
pub(super) fn mkdir_all(p: &str) -> Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o755).create(p)?;
    Ok(())
}

/// Creates or truncates `p` and writes `data` with `mode` (subject to umask).
pub(super) fn write_file(p: &str, data: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(p)?;
    f.write_all(data)
}

/// Reports whether two paths name the same place: lexically, or after resolving symlinks.
/// Case is ignored on macOS and Windows.
pub(super) fn same_path(a: &str, b: &str) -> bool {
    let a = path::clean(a);
    let b = path::clean(b);
    if path_equal(&a, &b) {
        return true;
    }
    let (Ok(ar), Ok(br)) = (std::fs::canonicalize(&a), std::fs::canonicalize(&b)) else {
        return false;
    };
    path_equal(&path::clean(&ar.to_string_lossy()), &path::clean(&br.to_string_lossy()))
}

fn path_equal(a: &str, b: &str) -> bool {
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        return a.to_lowercase() == b.to_lowercase();
    }
    a == b
}
