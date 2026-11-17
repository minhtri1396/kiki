//! Cloning a repository into the folder a repository kind uses.
//!
//! Pattern: Controller + Facade. [`CloneController`] registers the route and `service` runs the use case; `url`
//! checks the address, `folder` owns the folder rules (what counts as empty, the scratch folder, moving the
//! finished clone in) and `git` is the thin layer over the `git` program.

mod controller;
mod folder;
mod git;
mod service;
mod url;

#[cfg(test)]
mod tests;

pub(crate) use controller::CloneController;
pub(crate) use folder::folder_empty;
pub(crate) use git::git_origin;
