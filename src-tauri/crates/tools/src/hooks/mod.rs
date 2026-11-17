//! The RTK command hook inside each AI tool's hooks file: one dialect per tool (`dialect`), the shared check/remove
//! skeleton (`edit`), and the `rtk init` command lines (`steps`).

mod dialect;
mod edit;
mod steps;
#[cfg(test)]
mod tests;

pub(crate) use dialect::for_tool;
pub(crate) use steps::{install_hook_steps, install_steps, remove_hook_steps, run_steps};
