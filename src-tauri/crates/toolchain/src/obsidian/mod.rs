//! Pattern: Facade + Dependency Injection. `Obsidian` is the one entry point for vault status, open, registration and
//! the `obsidian` command link; it reads the machine only through an injected `ObsidianEnv`. The free functions
//! below use the production `SystemEnv`.

mod cli;
mod env;
mod fsutil;
mod install;
mod open;
mod registry;
#[cfg(test)]
mod tests;

pub use env::{ObsidianEnv, SystemEnv};

use anyhow::Result;
use kiki_util::path;
use std::sync::Arc;

/// Obsidian operations bound to one environment.
#[derive(Clone)]
pub struct Obsidian {
    env: Arc<dyn ObsidianEnv>,
}

impl Obsidian {
    /// Binds the operations to `env`.
    pub fn new(env: Arc<dyn ObsidianEnv>) -> Obsidian {
        Obsidian { env }
    }

    /// The production environment.
    pub fn system() -> Obsidian {
        Obsidian::new(Arc::new(SystemEnv))
    }

    /// Opens `folder` in Obsidian and waits until the CLI reports that path.
    /// The CLI only answers when that vault is open. "Vault not found." means no
    /// vault window is open, so this opens the folder and, if the CLI still cannot
    /// see it, relaunches Obsidian on that folder.
    pub fn open_vault(&self, cli: &str, folder: &str) -> Result<()> {
        open::open_vault(self.env.as_ref(), cli, folder)
    }

    /// Makes `folder` an Obsidian vault and registers it in the Obsidian vault list.
    /// The returned note is safe to show in the command log.
    pub fn ensure_vault(&self, folder: &str) -> Result<String> {
        registry::ensure_vault(self.env.as_ref(), folder)
    }

    /// The obsidian-cli binary inside the Obsidian app; empty when it is not installed.
    pub fn bundled_cli(&self) -> String {
        install::bundled_cli(self.env.as_ref())
    }

    /// Makes `obsidian` available without administrator rights.
    /// It links the bundled CLI into ~/.local/bin when that name is not already there.
    pub fn install_command(&self) -> Result<String> {
        install::install_command(self.env.as_ref())
    }

    /// The status command for `folder`.
    /// A shared vault name is left off: `vault=<name>` does not return when several vaults use it.
    pub fn status_args(&self, folder: &str) -> Vec<String> {
        if registry::shared_vault_name(self.env.as_ref(), folder) {
            return vec!["obsidian".to_string(), "vault".to_string(), "info=path".to_string()];
        }
        obsidian_status(folder)
    }
}

/// The Obsidian vault name for a folder: its last path segment.
/// The CLI parameter vault= takes this name, not the absolute path.
pub fn vault_name(folder: &str) -> String {
    path::base(&path::clean(folder))
}

/// `obsidian vault=<name> vault info=path`.
/// vault= is the first parameter so the CLI reads this vault, not another open one.
/// Use [`obsidian_check_args`] when the name might be shared: that form does not return.
pub fn obsidian_status(folder: &str) -> Vec<String> {
    vec![
        "obsidian".to_string(),
        format!("vault={}", vault_name(folder)),
        "vault".to_string(),
        "info=path".to_string(),
    ]
}

/// [`Obsidian::status_args`] on the production environment.
pub fn obsidian_check_args(folder: &str) -> Vec<String> {
    Obsidian::system().status_args(folder)
}

/// [`Obsidian::open_vault`] on the production environment.
pub fn open_vault(cli: &str, folder: &str) -> Result<()> {
    Obsidian::system().open_vault(cli, folder)
}

/// [`Obsidian::ensure_vault`] on the production environment.
pub fn ensure_vault(folder: &str) -> Result<String> {
    Obsidian::system().ensure_vault(folder)
}

/// [`Obsidian::bundled_cli`] on the production environment.
pub fn bundled_obsidian_cli() -> String {
    Obsidian::system().bundled_cli()
}

/// [`Obsidian::install_command`] on the production environment.
pub fn install_obsidian_command() -> Result<String> {
    Obsidian::system().install_command()
}
