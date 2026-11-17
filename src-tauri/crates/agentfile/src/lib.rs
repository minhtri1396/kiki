//! Stores each agent as a markdown file with a YAML front matter in the agent folder.
//!
//! * `model`: the file types and typed errors.
//! * `parse` / `serialize`: front matter to and from [`File`], on the shared `kiki-yaml` reader and emitter.
//! * `normalize`: validation before a save.
//! * `folder`: list, find, save and resolve; `catalog`: the embedded starter agents; `disk`: file system helpers.

mod catalog;
mod disk;
mod folder;
mod model;
mod normalize;
mod parse;
mod serialize;

pub use catalog::ensure;
pub use folder::{find, list, replace_repos, resolve, save, Replacements};
pub use model::{
    check_handoffs, is_exists, is_not_found, Collaborator, ErrExists, ErrNotFound, File, Handoff, Repository, Use,
    MAX_HANDOFF_LOOPS,
};

#[cfg(test)]
mod tests;
