//! Guardrails inside the app: which agent may do what, the hooks that ask the app, and the folders it learns about.
//!
//! Pattern: Controller + Facade. [`GuardController`] answers the hooks (`check` decides with the policy of the
//! agent that applies); `hooks` installs and removes the AI tool hooks; [`GuardBinder`] is the cheap handle a
//! launch uses to tell the guardrail which worktree an agent runs in; `policy` turns an agent file into a policy.

mod binder;
mod check;
mod controller;
mod hooks;
mod policy;

#[cfg(test)]
mod tests;

pub(crate) use controller::GuardController;
pub(crate) use policy::use_ids;
