//! What the manager needs from the machine, injected so tests can replace it.

use crate::process::path_value;
use anyhow::{anyhow, Result};
use kiki_util::path;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;

/// Finds a command on the command PATH.
pub(crate) type LookPathFn = Arc<dyn Fn(&str) -> Result<String> + Send + Sync>;

/// Dependency injection for the manager and the index tools.
#[derive(Clone)]
pub(crate) struct Env {
    /// Resolves a command name to an executable path.
    pub(crate) look_path: LookPathFn,
    /// Obsidian's CLI answers `Command "vault" not found` while the app is still starting;
    /// the check waits this long and tries once more.
    pub(crate) obsidian_retry_delay: Duration,
}

impl Default for Env {
    fn default() -> Self {
        Env {
            look_path: Arc::new(look_path),
            obsidian_retry_delay: Duration::from_millis(3000),
        }
    }
}

/// Searches the command PATH (with the extra tool folders in front). `obsidian` also falls back to
/// the CLI bundled in the Obsidian app.
fn look_path(name: &str) -> Result<String> {
    for dir in path_value().split(':') {
        let candidate = path::join(&[dir, name]);
        if executable_file(&candidate) {
            return Ok(candidate);
        }
    }
    if name == "obsidian" {
        let bundled = kiki_toolchain::bundled_obsidian_cli();
        if !bundled.is_empty() {
            return Ok(bundled);
        }
    }
    Err(anyhow!("{name} not found"))
}

fn executable_file(path: &str) -> bool {
    std::fs::metadata(path)
        .map(|info| !info.is_dir() && info.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
