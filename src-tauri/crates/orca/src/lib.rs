//! Starts an embedded KIKI agent in the Orca app.
//!
//! `client` is the facade; `launch`, `terminal`, `cleanup`, `projects`, `groups`, `worktrees` are its collaborators,
//! `cli` and `transport` reach Orca, `names`/`wire`/`paths` are shared helpers, `transcript` reads agent screens.

mod backend;
mod cleanup;
mod cli;
mod client;
mod groups;
mod launch;
mod names;
mod paths;
mod projects;
mod terminal;
mod transcript;
mod transport;
mod wire;
mod worktrees;

pub use cleanup::TaskScope;
pub use client::{Client, WorktreeFn};
pub use launch::{launch_args, FeatureSpec, LaunchBuilder, LaunchSpec};
pub use names::{feature_branch, feature_project_name, project_name, valid_branch_slug, valid_task_id, valid_ticket};
pub use terminal::{terminal_handle, TerminalView};
pub use transcript::{transcript, Ask, Question};
pub use transport::{FnTransport, RuntimeFn, RuntimeTransport, SocketTransport};

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    static ENV: Mutex<()> = Mutex::new(());

    /// Serializes tests that change process environment variables read by production code.
    pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
        ENV.lock().unwrap_or_else(|e| e.into_inner())
    }
}
