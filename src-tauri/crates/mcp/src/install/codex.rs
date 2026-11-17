//! Codex keeps MCP servers in `~/.codex/config.toml`. The file is edited as text, never re-serialized, so
//! comments and formatting the user wrote survive.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use super::writer::{InstallRequest, McpWriter};

pub(super) struct CodexWriter;

pub(super) static CODEX: CodexWriter = CodexWriter;

impl McpWriter for CodexWriter {
    fn label(&self) -> &'static str {
        "Codex"
    }

    fn user_config(&self, home: &Path) -> PathBuf {
        home.join(".codex").join("config.toml")
    }

    fn project_config(&self, worktree: &Path) -> PathBuf {
        worktree.join(".codex").join("config.toml")
    }

    fn project_file_has_key(&self, data: &[u8], key: &str) -> bool {
        toml_has_key(&String::from_utf8_lossy(data), key)
    }

    fn user_file_has_key(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<bool> {
        Ok(toml_has_key(&String::from_utf8_lossy(data), request.key))
    }

    fn merge(&self, data: &[u8], request: &InstallRequest<'_>) -> Result<Vec<u8>> {
        let block = codex_block(request.key, request.env)?;
        if String::from_utf8_lossy(data).trim().is_empty() {
            return Ok(block.strip_prefix('\n').unwrap_or(&block).as_bytes().to_vec());
        }
        let mut next = data.to_vec();
        if !data.ends_with(b"\n") {
            next.push(b'\n');
        }
        next.extend_from_slice(block.as_bytes());
        Ok(next)
    }
}

/// The TOML tables for one server; env entries come out in sorted key order.
pub(super) fn codex_block(key: &str, env: &BTreeMap<String, String>) -> Result<String> {
    let mut b = format!("\n[mcp_servers.{key}]\ncommand = \"uvx\"\nargs = [\"mcp-atlassian\"]\n");
    if env.is_empty() {
        return Ok(b);
    }
    b.push_str(&format!("\n[mcp_servers.{key}.env]\n"));
    for (name, value) in env {
        let quoted = toml_basic(value)?;
        b.push_str(&format!("{name} = {quoted}\n"));
    }
    Ok(b)
}

/// A TOML basic string; control characters cannot be stored.
pub(super) fn toml_basic(value: &str) -> Result<String> {
    if value.chars().any(|c| (c as u32) < 0x20) {
        return Err(anyhow!("an Atlassian value contains a character that cannot be stored"));
    }
    let value = value.replace('\\', "\\\\").replace('"', "\\\"");
    Ok(format!("\"{value}\""))
}

/// Whether a `[mcp_servers.<key>]` (or `.env`) table header is in the text, ignoring case and quotes.
pub(super) fn toml_has_key(text: &str, key: &str) -> bool {
    let want = format!("mcp_servers.{}", key.to_lowercase());
    for line in text.split('\n') {
        let trim = line.trim();
        if trim.is_empty() || trim.starts_with('#') {
            continue;
        }
        if !trim.starts_with('[') || !trim.ends_with(']') {
            continue;
        }
        let section = trim[1..trim.len() - 1]
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_lowercase();
        if section == want || section == format!("{want}.env") {
            return true;
        }
    }
    false
}
