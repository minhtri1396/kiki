//! Pattern: Facade. `Client` is the one entry point to Orca for the server; each call builds the small
//! collaborators it needs (`Cli`, `Projects`, `Groups`, `Worktrees`, `Terminals`, `Cleanup`) over the client's settings.

use crate::backend::Backend;
use crate::cleanup::{Cleanup, TaskScope};
use crate::launch::{LaunchSpec, Launcher};
use crate::terminal::{TerminalView, Terminals};
use crate::transport::{FnTransport, RuntimeFn, RuntimeTransport, SocketTransport};
use anyhow::Result;
use kiki_util::Ctx;
use std::sync::Arc;

/// Called with the folder an agent will run in, before that agent starts.
pub type WorktreeFn = Arc<dyn Fn(&str) + Send + Sync>;

/// Client talks to the local Orca CLI.
#[derive(Clone, Default)]
pub struct Client {
    pub bin: String,
    pub dir: String,
    /// Env is added to the environment of Orca commands (`KEY=VALUE` entries).
    pub env: Vec<String>,
    /// OnWorktree is called with the folder an agent will run in, before that agent starts.
    pub on_worktree: Option<WorktreeFn>,
    /// Runtime answers an Orca runtime method. When unset, the local Orca app is used.
    pub runtime: Option<RuntimeFn>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("bin", &self.bin)
            .field("dir", &self.dir)
            .field("env", &self.env)
            .field("on_worktree", &self.on_worktree.is_some())
            .field("runtime", &self.runtime.is_some())
            .finish()
    }
}

impl Client {
    /// Launch opens the repo folder in the named Orca project, starts the agent, and titles the tab.
    /// With a feature in the spec it first puts the project in the group folder, creates the task
    /// worktree from that repo, and starts the agent in the worktree folder.
    pub fn launch(&self, ctx: &Ctx, spec: &LaunchSpec) -> Result<String> {
        Launcher::new(self).launch(ctx, spec)
    }

    /// Opens one repository in its Orca project and returns the task worktree created from that git repo.
    pub fn ensure_task_worktree(
        &self,
        ctx: &Ctx,
        root: &str,
        project: &str,
        group: &str,
        name: &str,
        branch: &str,
    ) -> Result<String> {
        Launcher::new(self).ensure_task_worktree(ctx, root, project, group, name, branch)
    }

    /// ReadTerminal reads what an Orca terminal is showing, and whether it is still running.
    pub fn read_terminal(&self, ctx: &Ctx, handle: &str) -> Result<TerminalView> {
        Terminals::new(self.backend().cli).read(ctx, handle)
    }

    /// TerminalIdle reports whether the terminal's text interface has stopped updating.
    pub fn terminal_idle(&self, ctx: &Ctx, handle: &str) -> Result<bool> {
        Terminals::new(self.backend().cli).idle(ctx, handle)
    }

    /// SendText submits a reply to the agent in an Orca terminal.
    pub fn send_text(&self, ctx: &Ctx, handle: &str, text: &str) -> Result<()> {
        Terminals::new(self.backend().cli).send_text(ctx, handle, text)
    }

    /// RemoveTask closes the task's agent terminals and removes its worktrees.
    /// The group folder and repo projects stay when another task in the group still uses them.
    pub fn remove_task(&self, ctx: &Ctx, scope: &TaskScope) -> Result<()> {
        Cleanup::new(self.backend()).remove_task(ctx, scope)
    }

    /// The runtime transport: the injected closure (adapted), or the local Orca app's socket.
    pub(crate) fn transport(&self) -> Arc<dyn RuntimeTransport> {
        match &self.runtime {
            Some(f) => Arc::new(FnTransport(f.clone())),
            None => Arc::new(SocketTransport),
        }
    }

    fn backend(&self) -> Backend {
        Backend::new(self)
    }

    /// Tells the owner of `on_worktree` which folder an agent is about to run in.
    pub(crate) fn note_worktree(&self, path: &str) {
        if let Some(f) = &self.on_worktree {
            if !path.trim().is_empty() {
                f(path);
            }
        }
    }
}
