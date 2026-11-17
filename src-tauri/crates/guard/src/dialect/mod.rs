//! Pattern: Strategy + Factory Method + Template Method. One [`HookDialect`] per `AiTool` holds what differs
//! (settings file, hook entry shape, events, reply format); [`dialect`] picks it; the install, remove and ready
//! flows are provided methods of the trait that call those differences.

mod claude;
mod codex;
mod cursor;
mod gemini;
mod layout;

use std::path::Path;

use anyhow::{anyhow, bail};
use kiki_aitool::AiTool;
use serde_json::{Map, Value};

pub use layout::HookLayout;
use layout::MARKER;

use crate::settings::{self, Root};

/// What differs between AI tools in their guardrail hooks.
///
/// Implementors describe the tool; the provided `install`, `remove` and `ready` methods are the shared flows:
/// read the settings file, change our entries, write it back atomically.
pub trait HookDialect: Sync {
    /// The AI tool this dialect belongs to.
    fn tool(&self) -> AiTool;

    /// The name shown in error messages.
    fn title(&self) -> &'static str;

    /// The settings file that holds hooks, relative to the home folder.
    fn settings_file(&self) -> &'static str;

    /// The hook events KIKI installs a command for.
    fn events(&self) -> &'static [&'static str];

    /// How an event's list is laid out in the settings file.
    fn layout(&self) -> HookLayout;

    /// The entry that runs `command` for one event.
    fn entry(&self, command: &str) -> Value;

    /// Settings this tool needs at the top of the file, added only when missing.
    fn prepare_root(&self, _root: &mut Root) {}

    /// The reply text (without trailing newline) for a hook `event`. `message` is never empty when `allow` is false.
    fn reply_body(&self, event: &str, allow: bool, message: &str) -> String;

    /// Template method. Writes `command` as the hook for every event; entries that are not ours stay.
    fn install(&self, home: &Path, command: &str) -> anyhow::Result<()> {
        let path = home.join(self.settings_file());
        let mut root = settings::read_root(&path)?;
        self.prepare_root(&mut root);
        let mut hooks = settings::take_hooks(&mut root)?.unwrap_or_default();
        for event in self.events() {
            let mut list = self
                .layout()
                .drop_ours(hooks.get(*event).unwrap_or(&Value::Null))
                .map_err(|e| anyhow!("{event}: {e}"))?;
            list.push(self.entry(command));
            hooks.insert(event.to_string(), Value::Array(list));
        }
        root.insert("hooks".to_string(), Value::Object(hooks));
        settings::write_root(&path, root)
    }

    /// Template method. Deletes our hooks and rewrites the file only when something changed.
    fn remove(&self, home: &Path) -> anyhow::Result<()> {
        let path = home.join(self.settings_file());
        let Some(raw) = settings::read_optional(&path)? else {
            return Ok(());
        };
        if settings::is_blank(&raw) {
            return Ok(());
        }
        let mut root = settings::parse_root(&raw)?;
        let Some(mut hooks) = settings::take_hooks(&mut root)? else {
            return Ok(());
        };
        if !strip_ours(self.layout(), &mut hooks)? {
            return Ok(());
        }
        root.insert("hooks".to_string(), Value::Object(hooks));
        settings::write_root(&path, root)
    }

    /// Template method. Fails unless every event has our hook for this tool.
    fn ready(&self, home: &Path) -> anyhow::Result<()> {
        let not_installed = || anyhow!("{} guardrail hooks are not installed", self.title());
        let raw = std::fs::read(home.join(self.settings_file())).map_err(|_| not_installed())?;
        let mut root = settings::parse_root(&raw).map_err(|_| not_installed())?;
        let hooks = match root.remove("hooks") {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        };
        let suffix = format!("{MARKER}{}", self.tool().id());
        for event in self.events() {
            if !self.layout().has_command(hooks.get(*event), &suffix) {
                bail!("{} guardrail hook {} is missing", self.title(), event);
            }
        }
        Ok(())
    }
}

/// Removes our entries from every event; events left empty are dropped. Returns whether anything changed.
fn strip_ours(layout: HookLayout, hooks: &mut Map<String, Value>) -> anyhow::Result<bool> {
    let mut changed = false;
    let events: Vec<String> = hooks.keys().cloned().collect();
    for event in events {
        let value = hooks.get(&event).cloned().unwrap_or(Value::Null);
        let next = layout.drop_ours(&value).map_err(|e| anyhow!("{event}: {e}"))?;
        // An emptied list is the same as no list: both read as `null`.
        let before = serde_json::to_string(&value)?;
        let after = if next.is_empty() {
            "null".to_string()
        } else {
            serde_json::to_string(&Value::Array(next.clone()))?
        };
        changed |= before != after;
        if next.is_empty() {
            hooks.remove(&event);
        } else {
            hooks.insert(event, Value::Array(next));
        }
    }
    Ok(changed)
}

/// The dialect of an AI tool.
pub fn dialect(tool: AiTool) -> &'static dyn HookDialect {
    match tool {
        AiTool::Cursor => &cursor::Cursor,
        AiTool::Claude => &claude::Claude,
        AiTool::Codex => &codex::Codex,
        AiTool::Gemini => &gemini::Gemini,
    }
}

#[cfg(test)]
mod tests;
