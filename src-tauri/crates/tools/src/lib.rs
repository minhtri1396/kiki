//! The CLI tools Settings can turn on. A global tool applies to every agent.
//! The others are chosen on each agent.

mod hooks;
mod system;
mod toolbox;
mod tools;

pub use system::{RealSystem, RunOutput, System};
pub use toolbox::{ensure_hook, hook_ready, installed, remove_hooks, sync_hooks, Toolbox};
pub use tools::{catalog, global, Settings, Spec};
