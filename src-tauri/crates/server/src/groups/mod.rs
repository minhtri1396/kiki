//! Task groups: a group of tasks, optionally backed by a Jira epic that is created and deleted with it.
//!
//! Pattern: Controller + service. [`GroupController`] registers the routes; `service` holds the Jira epic rules.

mod controller;
mod service;

#[cfg(test)]
mod tests;

pub(crate) use controller::GroupController;
