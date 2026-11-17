//! The AI tools the console can start, as ids and as [`AiTool`].

use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;

use kiki_aitool::AiTool;

use crate::error::input_error;

/// The runtimes `orca agent launch --agent` accepts in this console.
///
/// The same ids as [`AiTool::ALL`], in the same order (a test keeps them equal). It stays a slice of
/// ids because the web UI and the setup files use the plain strings.
pub const ORCA_AGENTS: &[&str] = &["cursor", "claude", "codex", "gemini"];

/// A model name a CLI accepts, including Cursor's bracket overrides.
pub(crate) static MODEL_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._:@+=,\[\]-]{0,160}$").unwrap());

/// Parses a tool id at the edge (spaces around it are ignored).
/// An unknown id is an [`InputError`](crate::InputError) with the text "AI Tool is not supported".
pub fn parse_tool(id: &str) -> Result<AiTool> {
    AiTool::parse(id.trim()).ok_or_else(|| input_error("AI Tool is not supported"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InputError;

    #[test]
    fn orca_agents_are_the_ai_tools() {
        assert_eq!(ORCA_AGENTS, AiTool::ids().as_slice());
    }

    #[test]
    fn parse_tool_trims_and_rejects_unknown_ids() {
        assert_eq!(parse_tool(" claude ").unwrap(), AiTool::Claude);
        let err = parse_tool("vim").unwrap_err();
        assert_eq!(err.to_string(), "AI Tool is not supported");
        assert!(err.downcast_ref::<InputError>().is_some());
        assert!(parse_tool("Claude").is_err(), "ids are case-sensitive");
    }
}
