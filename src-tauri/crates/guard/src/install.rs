//! Installs, removes, and checks the guardrail hooks of every AI tool. Each tool's own steps live in its
//! [`HookDialect`](crate::HookDialect); this is the entry point that picks the dialect and joins errors.

use std::path::Path;

use anyhow::bail;
use kiki_aitool::AiTool;

use crate::command::hook_command;
use crate::dialect::dialect;

/// Writes command hooks for every AI tool. Hooks already configured for something else stay in place.
/// A failing tool does not stop the others; their errors are joined.
pub fn install(home: impl AsRef<Path>, exe: &str) -> anyhow::Result<()> {
    each_tool(|tool| install_tool(home.as_ref(), exe, tool))
}

/// Deletes the guardrail hooks of every AI tool. Hooks already configured for something else stay in place.
pub fn remove(home: impl AsRef<Path>) -> anyhow::Result<()> {
    each_tool(|tool| remove_tool(home.as_ref(), tool))
}

/// Writes the guardrail hooks for one AI tool, run before input goes to the model and after the model output.
pub fn install_tool(home: impl AsRef<Path>, exe: &str, tool: AiTool) -> anyhow::Result<()> {
    let command = format!("{} {}", hook_command(exe), tool.id());
    dialect(tool).install(home.as_ref(), &command)
}

/// Deletes the guardrail hooks for one AI tool.
pub fn remove_tool(home: impl AsRef<Path>, tool: AiTool) -> anyhow::Result<()> {
    dialect(tool).remove(home.as_ref())
}

/// Reports that this AI tool has every guardrail hook KIKI installs.
pub fn hooks_ready(home: impl AsRef<Path>, tool: AiTool) -> anyhow::Result<()> {
    dialect(tool).ready(home.as_ref())
}

fn each_tool(mut step: impl FnMut(AiTool) -> anyhow::Result<()>) -> anyhow::Result<()> {
    let errs: Vec<String> = AiTool::ALL
        .into_iter()
        .filter_map(|tool| {
            step(tool)
                .err()
                .map(|err| format!("{}: {err:#}", dialect(tool).title()))
        })
        .collect();
    if !errs.is_empty() {
        bail!("{}", errs.join("; "));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
