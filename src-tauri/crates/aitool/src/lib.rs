//! The AI tools an agent can run: Cursor, Claude, Codex, and Gemini.
//!
//! This enum is the single place that lists them. Every crate that behaves differently per tool
//! (guardrail hooks, MCP settings, RTK hook, launch command) matches on it **exhaustively**, so adding a
//! tool is a compile error everywhere it has to be wired up.
//!
//! Stored and serialized forms stay plain strings (`"cursor"`, ...); convert with [`AiTool::parse`] at the
//! edge and pass `AiTool` inside the program.

use std::fmt;
use std::str::FromStr;

/// An AI tool KIKI can start an agent in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiTool {
    Cursor,
    Claude,
    Codex,
    Gemini,
}

impl AiTool {
    /// Every tool, in the order Settings lists them.
    pub const ALL: [AiTool; 4] = [AiTool::Cursor, AiTool::Claude, AiTool::Codex, AiTool::Gemini];

    /// The id used in agent files, settings, and the web UI.
    pub fn id(self) -> &'static str {
        match self {
            AiTool::Cursor => "cursor",
            AiTool::Claude => "claude",
            AiTool::Codex => "codex",
            AiTool::Gemini => "gemini",
        }
    }

    /// The command-line program that runs the tool.
    pub fn binary(self) -> &'static str {
        match self {
            AiTool::Cursor => "cursor-agent",
            AiTool::Claude => "claude",
            AiTool::Codex => "codex",
            AiTool::Gemini => "gemini",
        }
    }

    /// Exact id match (`"cursor"`, not `" Cursor "`).
    pub fn parse(id: &str) -> Option<AiTool> {
        AiTool::ALL.into_iter().find(|tool| tool.id() == id)
    }

    /// Like [`AiTool::parse`] but ignores surrounding spaces and letter case.
    pub fn parse_loose(id: &str) -> Option<AiTool> {
        AiTool::parse(&id.trim().to_lowercase())
    }

    /// Ids of every tool, for validation lists and messages.
    pub fn ids() -> Vec<&'static str> {
        AiTool::ALL.iter().map(|tool| tool.id()).collect()
    }
}

impl fmt::Display for AiTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// An id that is not one of the supported tools.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownTool(pub String);

impl fmt::Display for UnknownTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown AI Tool {:?}", self.0)
    }
}

impl std::error::Error for UnknownTool {}

impl FromStr for AiTool {
    type Err = UnknownTool;

    fn from_str(id: &str) -> Result<AiTool, UnknownTool> {
        AiTool::parse(id).ok_or_else(|| UnknownTool(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for tool in AiTool::ALL {
            assert_eq!(AiTool::parse(tool.id()), Some(tool));
            assert_eq!(tool.to_string(), tool.id());
            assert_eq!(tool.id().parse::<AiTool>(), Ok(tool));
        }
    }

    #[test]
    fn parse_is_exact_and_loose_parse_is_forgiving() {
        assert_eq!(AiTool::parse(" Cursor "), None);
        assert_eq!(AiTool::parse_loose(" Cursor "), Some(AiTool::Cursor));
        assert_eq!(AiTool::parse_loose("nope"), None);
    }

    #[test]
    fn cursor_runs_cursor_agent() {
        assert_eq!(AiTool::Cursor.binary(), "cursor-agent");
        assert_eq!(AiTool::ids(), ["cursor", "claude", "codex", "gemini"]);
    }
}
