//! Installing and removing the hooks that make AI tools ask the app: the guardrail hooks and the RTK command hook.

use anyhow::Result;
use kiki_aitool::AiTool;
use kiki_guard as kg;
use kiki_tools as tools;

use crate::{tools_ready, LaunchPrep, Server};

impl Server {
    /// Makes sure the hooks Settings asks for exist for `tool` before an agent starts in it.
    pub(crate) fn ensure_agent_hooks(&self, tool: AiTool) -> Result<()> {
        let home = self.hook_home()?;
        let cfg = self.load_tools()?;
        if cfg.guardrails_on() {
            kg::install_tool(&home, &guard_executable()?, tool)?;
            kg::hooks_ready(&home, tool)?;
        }
        if !cfg.rtk {
            return Ok(());
        }
        self.ensure_rtk_hook(&home, tool)
    }

    /// Registers the RTK hook for `tool` when it is missing, then checks it is there.
    pub(crate) fn ensure_rtk_hook(&self, home: &str, tool: AiTool) -> Result<()> {
        if tools::hook_ready(home, tool).is_ok() {
            return Ok(());
        }
        match &self.rtk_hooks {
            Some(hooks) => hooks(home, true)?,
            None => tools::ensure_hook(home, tool)?,
        }
        tools::hook_ready(home, tool)
    }

    /// Fails when the agent selected a tool that Settings has not set up.
    pub(crate) fn ensure_agent_tools(&self, prep: &LaunchPrep) -> Result<()> {
        let wanted: Vec<String> = prep.agent.tools.iter().map(|item| item.id.clone()).collect();
        tools_ready(&wanted, &self.enabled_tools()?)
    }

    /// Publishes this app's address and installs or removes the AI tool hooks, following Settings.
    /// Guardrails stay installed when the setting is missing; turning them off removes the hooks.
    pub fn enable_guardrails(&self, addr: &str) -> Result<()> {
        let home = self.hook_home()?;
        let exe = guard_executable()?;
        let url = kg::listen_url(addr);
        let bindings = {
            let mut state = self.guard.lock().unwrap_or_else(|e| e.into_inner());
            state.home = home.clone();
            state.url = url.clone();
            state.bindings.clone()
        };
        self.guard_binder().publish(&home, &url, &bindings)?;
        let cfg = self.load_tools()?;
        self.apply_guardrails(&home, &exe, cfg.guardrails_on())
    }

    /// Applies a changed guardrails setting to the installed hooks.
    pub(crate) fn sync_guardrails(&self, on: bool) -> Result<()> {
        let home = self.hook_home()?;
        let exe = if on && self.guard_hooks.is_none() {
            guard_executable()?
        } else {
            String::new()
        };
        self.apply_guardrails(&home, &exe, on)
    }

    fn apply_guardrails(&self, home: &str, exe: &str, on: bool) -> Result<()> {
        if let Some(hooks) = &self.guard_hooks {
            return hooks(home, on);
        }
        if on {
            kg::install(home, exe)
        } else {
            kg::remove(home)
        }
    }
}

/// The path of this program as the hooks must call it back.
fn guard_executable() -> Result<String> {
    let exe = std::env::current_exe()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    Ok(exe.to_string_lossy().into_owned())
}
