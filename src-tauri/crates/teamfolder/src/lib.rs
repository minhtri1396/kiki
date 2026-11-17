//! Reads a local team folder. Each immediate subfolder is one member. That
//! member's agents live in an agents folder inside it. The user syncs the team
//! folder with their own tool.
//!
//! `teamfolder` is the public API, `paths` the name and path rules, `tree` reads and installs agent files,
//! `fsops` the small filesystem helpers.

mod fsops;
mod paths;
mod teamfolder;
mod tree;

#[cfg(test)]
mod tests;

pub use teamfolder::{
    agents_dir, copy, dir, has_agents, has_repos, member, members, repos_dir, same, user_dir, valid_name,
};
