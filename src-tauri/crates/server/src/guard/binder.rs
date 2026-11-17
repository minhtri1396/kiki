//! Telling the guardrail which worktree an agent runs in.
//!
//! Pattern: Facade. A [`GuardBinder`] owns just the shared pieces a binding needs (the store, the root, the guard
//! state), so the Orca callback that fires while an agent starts can hold it without borrowing the server.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use kiki_console as console;
use kiki_guard as kg;
use kiki_store::Store;
use kiki_util::path;

use super::policy::repo_folders;
use crate::{GuardState, Server};

/// Only the newest bindings are kept: agents that ended long ago must not widen what the guardrail allows.
const MAX_BINDINGS: usize = 40;

impl GuardState {
    /// Remembers one more agent worktree and forgets the oldest ones past the limit.
    fn push_binding(&mut self, binding: kg::Binding) {
        self.bindings.push(binding);
        if self.bindings.len() > MAX_BINDINGS {
            let extra = self.bindings.len() - MAX_BINDINGS;
            self.bindings.drain(..extra);
        }
    }
}

/// A cheap, thread-safe handle that binds agents to their worktree folders and publishes the result.
#[derive(Clone)]
pub(crate) struct GuardBinder {
    store: Arc<Store>,
    root: String,
    state: Arc<Mutex<GuardState>>,
}

impl GuardBinder {
    /// Records that `agent_id` runs in `worktree` and publishes the folders to the hooks. Nothing happens
    /// for a missing agent or a worktree that is not an absolute path, or before the app published an address.
    pub(crate) fn bind(&self, agent_id: &str, worktree: &str, readonly: bool, services: &[String]) {
        let (agent_id, worktree) = (agent_id.trim(), worktree.trim());
        if agent_id.is_empty() || worktree.is_empty() || !Path::new(worktree).is_absolute() {
            return;
        }
        let (home, url, bindings) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.push_binding(kg::Binding {
                agent_id: agent_id.to_string(),
                worktree: path::clean(worktree),
                read_only: readonly,
                services: services.to_vec(),
            });
            (state.home.clone(), state.url.clone(), state.bindings.clone())
        };
        if home.is_empty() || url.is_empty() {
            return;
        }
        let _ = self.publish(&home, &url, &bindings);
    }

    /// Writes the app address and every repository and worktree folder where the hooks read them.
    pub(crate) fn publish(&self, home: &str, url: &str, bindings: &[kg::Binding]) -> Result<()> {
        let snap = console::apply_root(&self.root, self.store.load()?)?;
        let mut paths = repo_folders(&snap);
        paths.extend(
            bindings
                .iter()
                .filter(|b| Path::new(&b.worktree).is_absolute())
                .map(|b| path::clean(&b.worktree)),
        );
        kg::publish(
            home,
            kg::Endpoint {
                url: url.to_string(),
                workspace: snap.workspace_id.clone(),
                paths,
            },
        )
    }
}

impl Server {
    /// The handle a launch uses to bind agents to worktrees.
    pub(crate) fn guard_binder(&self) -> GuardBinder {
        GuardBinder {
            store: self.store.clone(),
            root: self.root.clone(),
            state: self.guard.clone(),
        }
    }
}
