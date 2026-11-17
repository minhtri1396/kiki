//! Opens the workspace: finds the root directory, migrates and opens the database, writes the default
//! agent files and the git ignore rules, and builds the [`Server`].
//!
//! Pattern: Builder. `Workspace::open` runs the start-up steps in order and hands back the assembled parts.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use kiki_jobs::Manager;
use kiki_orca::Client;
use kiki_server::{ensure_agent_files, Server};
use kiki_store::Store;

/// Everything the shell needs after start-up.
pub struct Workspace {
    pub root: String,
    pub setup_path: String,
    pub agent_path: String,
    pub ignore_path: String,
    pub store: Arc<Store>,
    pub server: Arc<Server>,
}

impl Workspace {
    /// `root` overrides the directory (`--root`, then `KIKI_ROOT`); otherwise it is the executable's directory.
    pub fn open(root: Option<&str>) -> Result<Workspace> {
        let root = resolve_root(root)?;
        kiki_store::migrate(&root)?;
        let setup_path = kiki_store::path(&root);
        let store = Arc::new(Store::open(&setup_path)?);

        store.ensure_layout(&root)?;
        let agent_path = ensure_agent_files(&root)?;
        let entries = kiki_gitignore::app_entries(&root, &agent_path, &setup_path, store.log_file());
        let ignore_path = kiki_gitignore::ensure(&root, &entries)?;

        let server = Arc::new(Server::new(
            root.clone(),
            store.clone(),
            Client {
                dir: root.clone(),
                ..Default::default()
            },
            Arc::new(Manager::new()),
        ));
        Ok(Workspace {
            root,
            setup_path,
            agent_path,
            ignore_path,
            store,
            server,
        })
    }

    /// The start-up summary printed to the terminal.
    pub fn summary(&self, url: &str) -> String {
        let mut text = format!(
            "KIKI: {url}\nRepository: {}\nSetup: {}\nAgents: {}\nLogs: {}",
            self.root,
            self.setup_path,
            self.agent_path,
            self.store.log_file()
        );
        if !self.ignore_path.is_empty() {
            text.push_str(&format!("\nGit ignore: {}", self.ignore_path));
        }
        text
    }
}

fn resolve_root(explicit: Option<&str>) -> Result<String> {
    if let Some(root) = explicit.filter(|r| !r.is_empty()) {
        return absolute(PathBuf::from(root));
    }
    if let Some(root) = std::env::var_os("KIKI_ROOT").filter(|r| !r.is_empty()) {
        return absolute(PathBuf::from(root));
    }
    let exe = std::fs::canonicalize(std::env::current_exe()?)?;
    Ok(beside(&exe)?.to_string_lossy().into_owned())
}

fn absolute(path: PathBuf) -> Result<String> {
    let path = std::path::absolute(&path).with_context(|| format!("resolve {}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// The directory KIKI treats as "beside the app". For a macOS bundle that is the folder that holds
/// `KIKI.app`, not the inside of the bundle.
fn beside(exe: &std::path::Path) -> Result<PathBuf> {
    let dir = exe.parent().ok_or_else(|| anyhow!("no executable directory"))?;
    let mut ancestors = dir.ancestors();
    let in_bundle = dir.ends_with("Contents/MacOS");
    if in_bundle {
        if let Some(app) = ancestors.nth(2) {
            if let Some(holder) = app.parent() {
                return Ok(holder.to_path_buf());
            }
        }
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn plain_executable_uses_its_directory() {
        assert_eq!(beside(Path::new("/opt/kiki/kiki")).unwrap(), Path::new("/opt/kiki"));
    }

    #[test]
    fn bundled_executable_uses_the_folder_holding_the_app() {
        let exe = Path::new("/Users/me/work/KIKI.app/Contents/MacOS/kiki");
        assert_eq!(beside(exe).unwrap(), Path::new("/Users/me/work"));
    }

    #[test]
    fn explicit_root_wins_and_becomes_absolute() {
        let root = resolve_root(Some("/tmp/kiki-root")).unwrap();
        assert_eq!(root, "/tmp/kiki-root");
    }
}
