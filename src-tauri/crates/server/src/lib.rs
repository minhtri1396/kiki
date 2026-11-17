//! The KIKI API: `Server::handle` answers every request. Transports (Tauri IPC, loopback HTTP) adapt to it.

pub mod api_error;
pub mod http;

mod atlassian;
mod atlassian_list;
mod clone;
mod controller;
mod epic;
mod groups;
mod guard;
mod open;
mod runs;
mod server;
mod tasks;
mod team;

#[cfg(test)]
mod core_test;

pub use controller::Controller;
pub use server::*;
pub use team::FolderPicker;
