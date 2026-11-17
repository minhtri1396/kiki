//! Decides whether an agent may take the actions a hook reports.
//!
//! [`resolve`] picks the agent whose setup applies to a folder; [`Policy::decide`] allows or blocks its targets.
//! Pattern: Chain of Responsibility in both (`resolve::STEPS`, `decide::RULES`): each link decides or defers.

mod allow;
mod decide;
mod model;
mod paths;
mod resolve;
mod scope;
mod service;

#[cfg(test)]
mod tests;

pub use model::{Binding, Decision, Policy, Query, Resolution, Status, Target, TargetKind};
pub(crate) use paths::abs_path;
pub use paths::inside;
pub use resolve::resolve;
