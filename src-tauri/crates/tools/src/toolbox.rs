//! Pattern: Facade. [`Toolbox`] is the one entry point for the Settings tools: is a CLI installed, and install,
//! check, or remove the RTK hook in the AI tools. It reaches the machine only through an injected [`System`].

use std::sync::Arc;

use anyhow::{anyhow, Result};
use kiki_aitool::AiTool;

use crate::hooks::{for_tool, install_hook_steps, install_steps, remove_hook_steps, run_steps};
use crate::system::{RealSystem, System};

/// Tool and hook operations over an injected system. `Default` uses the real machine.
#[derive(Clone)]
pub struct Toolbox {
    system: Arc<dyn System>,
}

impl Default for Toolbox {
    fn default() -> Self {
        Toolbox::new(Arc::new(RealSystem))
    }
}

fn require_home(home: &str) -> Result<&str> {
    let home = home.trim();
    if home.is_empty() {
        return Err(anyhow!("home directory is not set"));
    }
    Ok(home)
}

impl Toolbox {
    pub fn new(system: Arc<dyn System>) -> Toolbox {
        Toolbox { system }
    }

    /// Reports that this command is on PATH.
    pub fn installed(&self, command: &str) -> bool {
        let command = command.trim();
        !command.is_empty() && self.system.command_exists(command)
    }

    /// Registers or removes the RTK command hook for every AI tool.
    /// The hook rewrites a shell command before it runs. The launch prompt does not mention RTK.
    /// Turning RTK off removes that hook from every AI tool this app supports.
    pub fn sync_hooks(&self, home: &str, on: bool) -> Result<()> {
        let home = require_home(home)?;
        if on {
            if !self.system.command_exists("rtk") {
                return Err(anyhow!("rtk is not installed"));
            }
            return run_steps(self.system.as_ref(), home, &install_hook_steps());
        }
        if self.system.command_exists("rtk") {
            let _ = run_steps(self.system.as_ref(), home, &remove_hook_steps());
        }
        self.remove_hooks(home)
    }

    /// Registers the RTK command hook for one AI tool.
    pub fn ensure_hook(&self, home: &str, tool: AiTool) -> Result<()> {
        let home = require_home(home)?;
        if !self.system.command_exists("rtk") {
            return Err(anyhow!("rtk is not installed"));
        }
        run_steps(self.system.as_ref(), home, &[install_steps(tool)])
    }

    /// Deletes RTK command hooks from every AI tool this app supports.
    /// Other hooks in those files stay in place.
    pub fn remove_hooks(&self, home: &str) -> Result<()> {
        let home = require_home(home)?;
        let mut errs: Vec<String> = Vec::new();
        for tool in AiTool::ALL {
            if let Err(err) = for_tool(tool).uninstall(home) {
                errs.push(format!("{err:#}"));
            }
        }
        if errs.is_empty() {
            return Ok(());
        }
        Err(anyhow!("RTK hook: {}", errs.join("; ")))
    }

    /// Reports that this AI tool has the RTK command hook.
    /// The hook rewrites a shell command before it runs.
    pub fn hook_ready(&self, home: &str, tool: AiTool) -> Result<()> {
        let home = require_home(home)?;
        for_tool(tool).ready(home)
    }
}

/// [`Toolbox::installed`] on the real machine.
pub fn installed(command: &str) -> bool {
    Toolbox::default().installed(command)
}

/// [`Toolbox::sync_hooks`] on the real machine.
pub fn sync_hooks(home: &str, on: bool) -> Result<()> {
    Toolbox::default().sync_hooks(home, on)
}

/// [`Toolbox::ensure_hook`] on the real machine.
pub fn ensure_hook(home: &str, tool: AiTool) -> Result<()> {
    Toolbox::default().ensure_hook(home, tool)
}

/// [`Toolbox::remove_hooks`] on the real machine.
pub fn remove_hooks(home: &str) -> Result<()> {
    Toolbox::default().remove_hooks(home)
}

/// [`Toolbox::hook_ready`] on the real machine.
pub fn hook_ready(home: &str, tool: AiTool) -> Result<()> {
    Toolbox::default().hook_ready(home, tool)
}
