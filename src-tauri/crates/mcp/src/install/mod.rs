//! Installs the Atlassian MCP server into an AI tool's user settings.
//!
//! Pattern: Factory Method + Strategy (see [`writer`]): the per-tool settings formats live in one writer per tool.
//! The only outside dependency, finding `uvx`, is injected into [`Installer`].

mod codex;
mod fs;
mod json_writers;
#[cfg(test)]
mod tests;
mod uvx;
mod writer;

use std::path::Path;
use std::sync::{Arc, LazyLock};

use anyhow::{anyhow, Result};
use kiki_aitool::AiTool;
use regex::Regex;

use crate::atlassian::Atlassian;
pub use uvx::LookPath;
use writer::{for_tool, InstallRequest};

static MCP_NAME_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").unwrap());
static WORKSPACE_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{0,31}$").unwrap());

/// ServerKey is the MCP server name written into an AI tool.
/// The name is mcp-<mcp name>-<workspace id>, for example mcp-atlassian-kiki.
pub fn server_key(name: &str, workspace: &str) -> Result<String> {
    let name = name.trim();
    let workspace = workspace.trim();
    if !MCP_NAME_PATTERN.is_match(name) {
        return Err(anyhow!("MCP name is not valid"));
    }
    if !WORKSPACE_KEY.is_match(workspace) {
        return Err(anyhow!(
            "workspace must be lowercase letters, digits, hyphens, or underscores"
        ));
    }
    Ok(format!("mcp-{name}-{workspace}"))
}

/// Ensure writes the Atlassian MCP server into the AI tool's user settings
/// when that tool does not already have the key mcp-atlassian-<workspace>.
/// A setup that is off does nothing. The same key in the launch folder, or in
/// Claude's settings for that folder, counts as set up.
pub fn ensure(tool: AiTool, worktree: &str, home: &str, workspace: &str, cfg: &Atlassian) -> Result<()> {
    Installer::default().ensure(tool, worktree, home, workspace, cfg)
}

/// Writes MCP servers into AI tool settings. Finding `uvx` goes through an injected lookup.
#[derive(Clone)]
pub struct Installer {
    look_path: LookPath,
}

impl Default for Installer {
    /// Looks for `uvx` on `PATH` and in the usual install folders.
    fn default() -> Self {
        Installer::with_look_path(Arc::new(uvx::find_uvx))
    }
}

impl Installer {
    /// An installer that finds programs with `look_path`.
    pub fn with_look_path(look_path: LookPath) -> Installer {
        Installer { look_path }
    }

    /// See [`ensure`].
    pub fn ensure(&self, tool: AiTool, worktree: &str, home: &str, workspace: &str, cfg: &Atlassian) -> Result<()> {
        let cfg = cfg.normalized();
        if !cfg.ready() {
            return Ok(());
        }
        cfg.validate()?;
        let home = home.trim();
        if home.is_empty() {
            return Err(anyhow!("home directory is not set"));
        }
        let key = server_key("atlassian", workspace)?;
        let writer = for_tool(tool);
        if writer.project_has_key(worktree, &key) {
            return Ok(());
        }
        let env = cfg.env();
        let request = InstallRequest {
            worktree,
            home: Path::new(home),
            key: &key,
            env: &env,
        };
        writer.install(&request, &|| self.require_uvx())
    }

    fn require_uvx(&self) -> Result<()> {
        if (self.look_path)("uvx").is_err() {
            return Err(anyhow!(
                "uvx is not installed. Atlassian MCP runs with uvx mcp-atlassian"
            ));
        }
        Ok(())
    }
}
