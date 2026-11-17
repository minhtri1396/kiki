//! Pattern: Template Method. Checking and removing a hook share one skeleton for every tool (read the hooks file,
//! parse it, look at or strip the RTK entry, write it back atomically); the dialect fills in the parts that differ.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use kiki_util::json;
use serde_json::{Map, Value};

use super::dialect::{HookDialect, HookSpec};

/// The top-level object of a hooks file; `None` when the file holds JSON `null`.
type Root = Option<Map<String, Value>>;

fn event_list<'a>(root: &'a Root, spec: &HookSpec) -> Option<&'a Value> {
    root.as_ref()
        .and_then(|r| r.get("hooks"))
        .and_then(|h| h.as_object())
        .and_then(|h| h.get(spec.event))
}

/// Reports that this tool has RTK's hook and, when the hook calls a script, that the script exists.
pub(super) fn ready<D: HookDialect + ?Sized>(dialect: &D, home: &str) -> Result<()> {
    let spec = dialect.spec();
    let title = spec.title;
    let not_installed = || anyhow!("{title} RTK hook is not installed");
    let raw = std::fs::read(Path::new(home).join(spec.config_rel)).map_err(|_| not_installed())?;
    let root: Root = match serde_json::from_slice::<Value>(&raw) {
        Ok(Value::Object(m)) => Some(m),
        Ok(Value::Null) => None,
        _ => return Err(not_installed()),
    };
    if !dialect.event_has_rtk(event_list(&root, spec)) {
        return Err(anyhow!("{title} RTK hook {} is missing", spec.event));
    }
    let Some(script) = spec.script else {
        return Ok(());
    };
    if std::fs::metadata(Path::new(home).join(script)).is_err() {
        return Err(anyhow!("{title} RTK hook script is missing"));
    }
    Ok(())
}

/// Removes RTK's hook entries and script files, keeping every other hook.
pub(super) fn uninstall<D: HookDialect + ?Sized>(dialect: &D, home: &str) -> Result<()> {
    let spec = dialect.spec();
    let script_gone = remove_script_files(home, spec)?;
    let path = Path::new(home).join(spec.config_rel);
    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if String::from_utf8_lossy(&raw).trim().is_empty() {
        return Ok(());
    }
    let mut root = parse_root(&raw, spec.title)?;
    let stripped = strip_events(dialect, &mut root, spec.title)?;
    if !(script_gone || stripped) {
        return Ok(());
    }
    let doc = match root {
        Some(m) => Value::Object(m),
        None => Value::Null,
    };
    write_hook_file(&path, json::marshal_indent(&doc).as_bytes())
}

fn parse_root(raw: &[u8], title: &str) -> Result<Root> {
    match serde_json::from_slice::<Value>(raw) {
        Ok(Value::Object(m)) => Ok(Some(m)),
        // A `null` document has no object to edit.
        Ok(Value::Null) => Ok(None),
        Ok(other) => Err(anyhow!(
            "{title} RTK hook: json: cannot unmarshal {} into Go value of type map[string]interface {{}}",
            json_kind(&other)
        )),
        Err(e) => Err(anyhow!("{title} RTK hook: {e}")),
    }
}

fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Strips RTK's entries from every event's hook list; an event left empty is dropped.
/// Returns whether anything was removed.
fn strip_events<D: HookDialect + ?Sized>(dialect: &D, root: &mut Root, title: &str) -> Result<bool> {
    let hooks_value = root.as_ref().and_then(|r| r.get("hooks"));
    let hooks_present = matches!(hooks_value, Some(v) if !v.is_null());
    if hooks_present && !matches!(hooks_value, Some(Value::Object(_))) {
        return Err(anyhow!("{title} RTK hook: hooks are not an object"));
    }
    let Some(hooks) = root
        .as_mut()
        .and_then(|r| r.get_mut("hooks"))
        .and_then(|h| h.as_object_mut())
    else {
        return Ok(false);
    };
    let mut changed = false;
    let events: Vec<String> = hooks.keys().cloned().collect();
    for event in events {
        let (next, removed) = match strip_list(dialect, &hooks[&event]) {
            Ok(r) => r,
            Err(err) => return Err(anyhow!("{title} {event}: {err}")),
        };
        if !removed {
            continue;
        }
        changed = true;
        if next.is_empty() {
            hooks.remove(&event);
        } else {
            hooks.insert(event, Value::Array(next));
        }
    }
    Ok(changed)
}

/// The event's hook list without RTK's entries, and whether anything was removed.
fn strip_list<D: HookDialect + ?Sized>(dialect: &D, value: &Value) -> Result<(Vec<Value>, bool)> {
    if value.is_null() {
        return Ok((Vec::new(), false));
    }
    let Value::Array(list) = value else {
        return Err(anyhow!("hooks are not a list"));
    };
    let mut out: Vec<Value> = Vec::new();
    let mut removed = false;
    for item in list {
        let (kept, item_removed) = dialect.strip_item(item);
        removed |= item_removed;
        out.extend(kept);
    }
    Ok((out, removed))
}

/// Deletes the script the hook calls and its leftovers; true when any file was removed.
fn remove_script_files(home: &str, spec: &HookSpec) -> Result<bool> {
    let Some(script) = spec.script else {
        return Ok(false);
    };
    let mut gone = false;
    for rel in std::iter::once(script).chain(spec.script_leftovers.iter().copied()) {
        match std::fs::remove_file(Path::new(home).join(rel)) {
            Ok(()) => gone = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(gone)
}

/// Writes the trimmed text plus one newline through a temp file and a rename.
fn write_hook_file(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o755).create(dir)?;
    }
    let mut body = String::from_utf8_lossy(data).trim().as_bytes().to_vec();
    body.push(b'\n');
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&tmp)?;
    f.write_all(&body)?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}
