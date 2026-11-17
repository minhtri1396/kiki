//! Turns the saved setup into the prompt sent when an agent starts, and rewrites text with an AI tool's CLI.
//!
//! - `snapshot`, `validate`, `services`, `orchestrator`: the saved setup and its checks.
//! - `prompt`: the launch prompt (Builder).
//! - `generate`: rewriting text with a CLI (Strategy per tool, Template Method for the run).
//! - `models`: the models each tool can launch (Strategy per tool).
//! - `host`: the one seam to the machine (installed CLIs, child processes), injected so tests need no globals.

mod diff;
mod error;
mod generate;
mod host;
mod models;
mod orchestrator;
mod prompt;
mod services;
mod snapshot;
mod text;
mod tool;
mod validate;

#[cfg(test)]
mod testing;

pub use diff::*;
pub use error::InputError;
pub use generate::*;
pub use host::{CliHost, CommandRun, SystemHost, WaitFn};
pub use models::*;
pub use orchestrator::*;
pub use prompt::*;
pub use services::scan_services;
pub use snapshot::*;
pub use tool::{parse_tool, ORCA_AGENTS};
pub use validate::validate;
