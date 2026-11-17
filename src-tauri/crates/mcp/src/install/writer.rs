//! Pattern: Strategy + Factory Method + Template Method. One [`McpWriter`] per AI tool knows where that tool keeps
//! its MCP settings and how to merge a server into them; [`for_tool`] picks it, and the provided
//! [`McpWriter::install`] runs the shared read -> check -> merge -> atomic write steps.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use kiki_aitool::AiTool;

use super::codex::CODEX;
use super::fs::{read_or_empty, write_private};
use super::json_writers::{CLAUDE, CURSOR, GEMINI};

/// What one install needs to know.
pub(super) struct InstallRequest<'a> {
    /// The launch folder; its own settings can already hold the server.
    pub worktree: &'a str,
    pub home: &'a Path,
    /// The server key, such as `mcp-atlassian-kiki`.
    pub key: &'a str,
    /// Environment for the server (`mcp-atlassian` reads its credentials from it).
    pub env: &'a BTreeMap<String, String>,
}

/// How one AI tool stores MCP servers.
pub(super) trait McpWriter: Sync {
    /// Display name used in error messages.
    fn label(&self) -> &'static str;
    /// The tool's user-level settings file.
    fn user_config(&self, home: &Path) -> PathBuf;
    /// The tool's settings file inside a launch folder.
    fn project_config(&self, worktree: &Path) -> PathBuf;
    /// Whether a launch-folder settings file already holds the key. A file that cannot be read counts as no.
    fn project_file_has_key(&self, data: &[u8], key: &str) -> bool;
    /// Whether the user-level settings already hold the key. Fails when the file cannot be understood,
    /// so it is never overwritten.
    fn user_file_has_key(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<bool>;
    /// The settings file text with the server added; everything else the user wrote stays as it was.
    fn merge(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<Vec<u8>>;

    /// Whether this launch folder already has the key. That server is available to the agent,
    /// so the user settings are left unchanged.
    fn project_has_key(&self, worktree: &str, key: &str) -> bool {
        if worktree.trim().is_empty() || key.is_empty() {
            return false;
        }
        let path = self.project_config(Path::new(worktree));
        match std::fs::read(path) {
            Ok(data) => self.project_file_has_key(&data, key),
            Err(_) => false,
        }
    }

    /// Template method: read the user settings, stop when the key is there, require `uvx`, merge, and write.
    fn install(&self, request: &InstallRequest<'_>, require_uvx: &dyn Fn() -> Result<()>) -> Result<()> {
        let label = self.label();
        let path = self.user_config(request.home);
        let data = read_or_empty(&path).map_err(|e| anyhow!("{label} MCP settings: {e}"))?;
        if self
            .user_file_has_key(&data, request)
            .map_err(|e| anyhow!("{label} MCP settings: {e}"))?
        {
            return Ok(());
        }
        require_uvx()?;
        let next = self.merge(&data, request)?;
        write_private(&path, &next).map_err(|e| anyhow!("{label} MCP settings: {e}"))
    }
}

/// Picks the writer for an AI tool.
pub(super) fn for_tool(tool: AiTool) -> &'static dyn McpWriter {
    match tool {
        AiTool::Cursor => &CURSOR,
        AiTool::Claude => &CLAUDE,
        AiTool::Codex => &CODEX,
        AiTool::Gemini => &GEMINI,
    }
}
