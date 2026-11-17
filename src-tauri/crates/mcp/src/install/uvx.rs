use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};

/// Finds a program by name and returns its path. Injected into the installer so tests can pretend
/// `uvx` is or is not installed.
pub type LookPath = Arc<dyn Fn(&str) -> Result<String> + Send + Sync>;

/// Looks on `PATH`, then in the usual install folders of a GUI app that does not inherit the shell's `PATH`.
pub(super) fn find_uvx(name: &str) -> Result<String> {
    if let Some(path) = kiki_util::look_path(name) {
        return Ok(path.to_string_lossy().into_owned());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        Path::new(&home).join(".local").join("bin"),
    ];
    for dir in candidates {
        let candidate = dir.join(name);
        if let Ok(info) = std::fs::metadata(&candidate) {
            if !info.is_dir() && info.permissions().mode() & 0o111 != 0 {
                return Ok(candidate.to_string_lossy().into_owned());
            }
        }
    }
    Err(anyhow!("uvx is not installed"))
}
