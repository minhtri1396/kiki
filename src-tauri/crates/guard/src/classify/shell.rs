//! Targets of a shell command: the files it names, the MCP servers it mentions, the agent it launches,
//! and the CLIs it runs.

use super::payload::{paths_of, Obj};
use super::tokens::{segment_command, split_command, tokenize};
use super::tools::mcp_mention;
use super::writes::command_writes;
use crate::policy::{Target, TargetKind};

/// The repository target for a command (it writes when `write` is set or the command writes), followed by
/// MCP, agent and tool targets found in it. `extra` are paths the payload named next to the command.
pub(super) fn shell_targets(command: &str, extra: Vec<String>, write: bool) -> Vec<Target> {
    let mut paths = extra;
    paths.extend(command_paths(command));
    let mut out = vec![Target::repo(paths, write || command_writes(command))];
    for name in mcp_mentions(command) {
        out.push(Target::named(TargetKind::Mcp, &name));
    }
    if let Some(name) = launched_agent(command) {
        out.push(Target::named(TargetKind::Agent, &name));
    }
    for name in command_tools(command) {
        out.push(Target::named(TargetKind::Tool, &name));
    }
    out
}

/// Shell targets for the `command`/`cmd` field of a payload.
pub(super) fn payload_shell_targets(m: &Obj, write: bool) -> Vec<Target> {
    shell_targets(&super::payload::command(m), paths_of(m), write)
}

fn command_tools(command: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if name.is_empty() || out.iter().any(|n| n == name) {
            return;
        }
        out.push(name.to_string());
    };
    for segment in split_command(command) {
        let tokens = tokenize(&segment);
        let (base, rest) = segment_command(&tokens);
        match base.as_str() {
            "gh" => add("GitHub CLI"),
            "rtk" => {
                add("RTK");
                let (next, _) = segment_command(rest);
                if next == "gh" {
                    add("GitHub CLI");
                }
            }
            _ => {}
        }
    }
    out
}

fn command_paths(command: &str) -> Vec<String> {
    let tokens = tokenize(command);
    let mut paths = Vec::new();
    for (i, tok) in tokens.iter().enumerate() {
        if tok == "cd" && i + 1 < tokens.len() {
            paths.push(tokens[i + 1].clone());
            continue;
        }
        if tok.starts_with('>') || tok.starts_with('<') {
            let target = tok.trim_start_matches(['<', '>']);
            if !target.is_empty() {
                paths.push(target.to_string());
            }
            continue;
        }
        if is_path_token(tok) {
            paths.push(tok.clone());
        }
    }
    paths
}

fn is_path_token(tok: &str) -> bool {
    if tok.is_empty() || tok.contains("://") {
        return false;
    }
    tok.starts_with('/') || tok.starts_with("./") || tok.starts_with("../") || tok.starts_with("~/") || tok == "~"
}

fn mcp_mentions(command: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for found in mcp_mention().find_iter(command) {
        let name = found.as_str().to_lowercase();
        if out.contains(&name) {
            continue;
        }
        out.push(name);
    }
    out
}

/// The `--agent` value of an `orca ... launch` command; `Some("")` when it launches without naming one.
fn launched_agent(command: &str) -> Option<String> {
    let low = command.to_lowercase();
    if !low.contains("orca") || !low.contains("launch") {
        return None;
    }
    let tokens = tokenize(command);
    for (i, tok) in tokens.iter().enumerate() {
        if tok == "--agent" && i + 1 < tokens.len() {
            return Some(tokens[i + 1].clone());
        }
    }
    Some(String::new())
}
