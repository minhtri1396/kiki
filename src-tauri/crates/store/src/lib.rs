//! Keeps the console setup in YAML files and run history and tasks in SQLite.
//!
//! Pattern: Facade. [`Store`] is the public entry point; its methods delegate to internal repositories:
//! `setup` (kiki.yaml, repos, tools and MCP files, legacy migration), `tasks`, `groups`, `runs` and
//! `launches`. `db` and `schema` hold the shared SQLite plumbing.

mod db;
mod fsutil;
mod groups;
mod launches;
mod runs;
mod schema;
mod setup;
mod store;
mod tasks;

pub use groups::{is_group_not_found, ErrGroupNotFound, Group, GroupInput};
pub use launches::{is_not_found, ErrNotFound, Launch};
pub use runs::{AskQuestion, RunUpdate, TaskRun};
pub use setup::{migrate, remove_index_folder, TeamFolder};
pub use store::{data_dir, detail_limit, open, path, Store};
pub use tasks::{
    is_task_not_found, AgentInput, AgentStatus, Column, ErrTaskNotFound, RunPhase, Task, TaskAgent, TaskError,
    TaskInput, UnknownValue,
};

#[cfg(test)]
mod tests;
