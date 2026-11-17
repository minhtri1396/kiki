//! Keeps Git from tracking files KIKI creates.
//!
//! `entries` lists what the app writes, `worktree` finds the repository and the paths inside it, `rules` reads
//! existing ignore rules, `file` writes the `.gitignore`, and `gitignore` ties them into `ensure` and `index`.

mod entries;
mod file;
mod gitignore;
mod rules;
mod worktree;

#[cfg(test)]
mod tests;

pub use entries::{app_entries, Entry};
pub use gitignore::{ensure, index};
