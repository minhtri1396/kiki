//! Small filesystem helpers shared by the team folder code.

use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

use anyhow::{anyhow, Result};
use kiki_util::path;
use sha2::{Digest, Sha256};

/// Creates `dir` and its parents with `mode`.
pub(super) fn mkdir_all(dir: &str, mode: u32) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(mode)
        .create(dir)
        .map_err(|e| anyhow!("mkdir {dir}: {e}"))
}

/// Lowercase hex SHA-256: the fingerprint the web UI compares between folders.
pub(super) fn hex_sha256(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// True for a real directory (a symlink to one does not count).
pub(super) fn is_real_dir(dir: &str) -> bool {
    match std::fs::symlink_metadata(dir) {
        Ok(info) => info.is_dir() && !info.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// Writes `body` to `file` and makes it world-readable (`0644`).
pub(super) fn write_shared_file(file: &str, body: &[u8]) -> Result<()> {
    std::fs::write(file, body).map_err(|e| anyhow!("open {file}: {e}"))?;
    let _ = std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644));
    Ok(())
}

/// A private temporary directory that is removed when dropped.
pub(super) struct TempDir(String);

impl TempDir {
    /// Creates `kiki-repos-<random>` under the system temp directory with mode `0700`.
    pub(super) fn create() -> Result<TempDir> {
        let base = std::env::temp_dir().to_string_lossy().into_owned();
        for _ in 0..10000 {
            let dir = path::join(&[&base, &format!("kiki-repos-{}", kiki_util::random_hex(4))]);
            match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => return Ok(TempDir(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(anyhow!("mkdir {dir}: {e}")),
            }
        }
        Err(anyhow!("pattern kiki-repos-: no usable temporary directory"))
    }

    pub(super) fn path(&self) -> &str {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
