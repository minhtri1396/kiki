//! Board tasks: create, update, delete (with the Orca cleanup) and the git branch name of a task.
//!
//! Pattern: Controller + service. [`TaskController`] registers the routes and stays thin; the rules live in
//! service methods on `Server` (`input` checks the form, `cleanup` removes a task from Orca, `branch` names the
//! git branch) that never see a `Request`.

mod branch;
mod cleanup;
mod controller;
mod error;
mod input;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

pub(crate) use controller::TaskController;
pub(crate) use error::{null_default, task_error, task_id, user_error};
