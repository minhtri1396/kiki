//! Runs Obsidian and CodeGraph setup, and checks whether they are ready.
//!
//! `index` holds one strategy per index tool, `manager` the shared job table, `runner` the background run,
//! `checks` the status checks, `process`/`text` the command and output helpers.

mod checks;
mod env;
mod index;
mod manager;
mod model;
mod process;
mod runner;
mod state;
mod text;

#[cfg(test)]
mod tests;

pub use manager::Manager;
pub use model::{Check, Job, JobStatus, Report, Target};
pub use text::format_args;
