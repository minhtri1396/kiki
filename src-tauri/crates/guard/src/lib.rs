//! Guardrails for AI tool hooks.
//!
//! An AI tool starts `kiki guardrail <tool>` before it sends input to the model and around every action it takes.
//! [`hook`] asks the running app, [`classify`] turns the payload into targets, and [`policy`] allows or blocks them.
//! [`dialect`] holds everything that differs per AI tool: where hooks are installed and how a reply looks.

mod classify;
mod command;
mod dialect;
mod endpoint;
mod hook;
mod install;
mod policy;
mod reply;
mod settings;
#[cfg(test)]
mod testutil;

pub use classify::classify;
pub use command::hook_command;
pub use dialect::{dialect, HookDialect, HookLayout};
pub use endpoint::{listen_url, load, publish, Endpoint};
pub use hook::{gate, reduce, run};
pub use install::{hooks_ready, install, install_tool, remove, remove_tool};
pub use policy::{inside, resolve, Binding, Decision, Policy, Query, Resolution, Status, Target, TargetKind};
pub use reply::reply;
