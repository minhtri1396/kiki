//! The `rtk init` command lines that install or remove the hook, and the runner that applies them.

use anyhow::{anyhow, Result};
use kiki_aitool::AiTool;

use super::dialect::for_tool;
use crate::system::System;

/// Order in which `sync_hooks(on)` installs the hook for every tool.
const INSTALL_ORDER: [AiTool; 4] = [AiTool::Claude, AiTool::Cursor, AiTool::Gemini, AiTool::Codex];
/// Order in which `sync_hooks(off)` asks rtk to uninstall; Cursor has no uninstall command.
const UNINSTALL_ORDER: [AiTool; 3] = [AiTool::Claude, AiTool::Gemini, AiTool::Codex];

fn to_args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The `rtk` arguments that register the hook for one tool.
pub(crate) fn install_steps(tool: AiTool) -> Vec<String> {
    to_args(for_tool(tool).spec().install_args)
}

/// Install steps for every tool.
pub(crate) fn install_hook_steps() -> Vec<Vec<String>> {
    INSTALL_ORDER.iter().map(|tool| install_steps(*tool)).collect()
}

/// Uninstall steps for the tools whose `rtk` can remove its own hook.
pub(crate) fn remove_hook_steps() -> Vec<Vec<String>> {
    UNINSTALL_ORDER
        .iter()
        .filter_map(|tool| for_tool(*tool).spec().uninstall_args)
        .map(to_args)
        .collect()
}

/// Runs each step, collecting every failure into one error.
pub(crate) fn run_steps(system: &dyn System, home: &str, steps: &[Vec<String>]) -> Result<()> {
    let mut errs: Vec<String> = Vec::new();
    for args in steps {
        let run = system.run_rtk(home, args);
        if let Err(err) = run.result {
            errs.push(format!("{}: {}", args.join(" "), hook_output(&run.output, &err)));
        }
    }
    if errs.is_empty() {
        return Ok(());
    }
    Err(anyhow!("RTK hook: {}", errs.join("; ")))
}

/// What rtk printed (at most 400 bytes), or the error when it printed nothing.
fn hook_output(out: &[u8], err: &anyhow::Error) -> String {
    let text = String::from_utf8_lossy(out).trim().to_string();
    if text.is_empty() {
        return err.to_string();
    }
    if text.len() > 400 {
        // Cut on bytes; a split character becomes the replacement character.
        return String::from_utf8_lossy(&text.as_bytes()[..400]).into_owned();
    }
    text
}
