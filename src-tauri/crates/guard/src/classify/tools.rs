//! What kind of action a tool name stands for, and whether a repository tool writes.

use regex::Regex;
use std::sync::OnceLock;

/// What an AI tool's own tool call does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActionKind {
    Mcp,
    Agent,
    Repo,
    Tool,
}

pub(super) fn mcp_mention() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)mcp[-_][a-z0-9][a-z0-9_-]*").expect("mcp mention pattern"))
}

pub(super) fn action_kind(name: &str) -> ActionKind {
    let n = name.trim().to_lowercase();
    if n.is_empty() {
        ActionKind::Tool
    } else if is_mcp(&n) {
        ActionKind::Mcp
    } else if is_agent_tool(&n) {
        ActionKind::Agent
    } else if is_repo_tool(&n) {
        ActionKind::Repo
    } else {
        ActionKind::Tool
    }
}

fn is_mcp(name: &str) -> bool {
    name.contains("mcp__") || name.contains("mcp_") || name.contains("mcp-") || name.starts_with("mcp:")
}

fn is_agent_tool(name: &str) -> bool {
    matches!(name, "task" | "agent" | "spawn_agent" | "subagent")
}

fn is_repo_tool(name: &str) -> bool {
    matches!(
        name,
        "shell"
            | "bash"
            | "read"
            | "write"
            | "edit"
            | "delete"
            | "grep"
            | "glob"
            | "multiedit"
            | "apply_patch"
            | "exec_command"
            | "run_shell_command"
            | "read_file"
            | "write_file"
            | "replace"
            | "list_directory"
            | "list_dir"
            | "grep_search"
            | "editnotebook"
            | "notebookedit"
            | "strreplace"
            | "search_replace"
            | "delete_file"
            | "read_many_files"
            | "ls"
    )
}

/// True for repository tools that can change files; read-only tools (shell tools decide by their command) are false.
pub(super) fn repo_tool_writes(name: &str) -> bool {
    !matches!(
        canonical_tool(name).as_str(),
        "shell"
            | "bash"
            | "execcommand"
            | "runshellcommand"
            | "read"
            | "grep"
            | "glob"
            | "listdirectory"
            | "listdir"
            | "grepsearch"
            | "readfile"
            | "readmanyfiles"
            | "ls"
    )
}

fn canonical_tool(name: &str) -> String {
    name.trim().to_lowercase().replace(['_', '-'], "")
}

/// The label an MCP target is known by: the tool name, the server, or both.
pub(super) fn mcp_label(server: &str, tool: &str) -> String {
    let server = server.trim();
    let tool = tool.trim();
    if server.is_empty() || server == tool || tool.contains(server) {
        return tool.to_string();
    }
    if tool.is_empty() {
        return server.to_string();
    }
    format!("{server} {tool}")
}

/// The first value that is not blank, trimmed; empty when all are blank.
pub(super) fn first_text(values: &[&str]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}
