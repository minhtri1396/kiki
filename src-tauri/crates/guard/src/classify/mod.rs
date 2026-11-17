//! Reads a hook payload and says which MCP servers, tools, repositories and agents the action touches.
//!
//! Pattern: Chain of Responsibility. A payload without a known event name goes through ordered inference rules
//! (`events::INFER`); each rule returns targets or defers to the next.

mod events;
mod git;
mod payload;
mod shell;
mod tokens;
mod tools;
mod writes;

#[cfg(test)]
mod tests;

use crate::policy::Target;

pub(crate) use payload::{field, object};

/// Reads a hook payload and returns the MCP, tool, repository, and agent interactions it is about to perform.
/// A prompt or model message with no such interaction returns no targets.
pub fn classify(event: &str, raw: &[u8]) -> Vec<Target> {
    let m = object(raw);
    let mut event = event.trim().to_lowercase();
    if event.is_empty() {
        event = field(&m, &["hook_event_name", "hookEventName", "event", "hook_event"]).to_lowercase();
    }
    events::targets(&event, &m)
}
