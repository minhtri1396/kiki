//! Agent runs of a task: polling the Orca terminals, replying to an agent and approving its work.
//!
//! Pattern: Controller + State. [`RunController`] registers the routes; `status` holds the closed set of run
//! statuses and the rules that pick one; `sync` applies what Orca shows to the store; `review` is the
//! user's side (reply, approve) and the record a launch leaves behind.

mod controller;
mod review;
mod status;
mod sync;

#[cfg(test)]
mod tests;

pub(crate) use controller::RunController;
