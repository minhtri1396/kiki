//! Finding the bundled Obsidian CLI and linking it as `obsidian`.

use super::env::ObsidianEnv;
use super::fsutil::{mkdir_all, same_path};
use anyhow::{anyhow, Result};
use kiki_util::path;
use std::os::unix::fs::PermissionsExt;

pub(super) fn bundled_cli(env: &dyn ObsidianEnv) -> String {
    env.cli_candidates()
        .into_iter()
        .find(|candidate| {
            matches!(std::fs::metadata(candidate), Ok(info) if !info.is_dir() && info.permissions().mode() & 0o111 != 0)
        })
        .unwrap_or_default()
}

pub(super) fn install_command(env: &dyn ObsidianEnv) -> Result<String> {
    let bundled = bundled_cli(env);
    if bundled.is_empty() {
        return Err(anyhow!(
            "Obsidian is not installed. Install it from https://obsidian.md/ and turn on Settings → General → Command line interface"
        ));
    }
    let dir = env.link_dir();
    if dir.trim().is_empty() || mkdir_all(&dir).is_err() {
        return Ok(bundled);
    }
    let dest = path::join(&[&dir, "obsidian"]);
    if same_path(&dest, &bundled) {
        return Ok(dest);
    }
    if std::fs::symlink_metadata(&dest).is_ok() || std::os::unix::fs::symlink(&bundled, &dest).is_err() {
        return Ok(bundled);
    }
    Ok(dest)
}
