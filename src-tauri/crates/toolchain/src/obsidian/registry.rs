//! The vault list: making a folder a vault and registering it in Obsidian's `obsidian.json`.

use super::env::ObsidianEnv;
use super::fsutil::{abs_path, mkdir_all, same_path, write_file};
use anyhow::{anyhow, Result};
use kiki_util::path;
use serde_json::{Map, Value};
use std::os::unix::fs::PermissionsExt;

pub(super) fn ensure_vault(env: &dyn ObsidianEnv, folder: &str) -> Result<String> {
    let folder = abs_path(folder)?;
    match std::fs::metadata(&folder) {
        Ok(info) if info.is_dir() => {}
        _ => return Err(anyhow!("folder {folder} does not exist")),
    }
    mkdir_all(&path::join(&[&folder, ".obsidian"]))?;
    let marker = path::join(&[&folder, ".obsidian", "app.json"]);
    match std::fs::metadata(&marker) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => write_file(&marker, b"{}\n", 0o644)?,
        Err(e) => return Err(e.into()),
    }
    register_vault(env, &folder)
}

/// The config file's top-level object and its permission bits (0644 for a new file).
struct Config {
    doc: Map<String, Value>,
    mode: u32,
}

fn read_config(file: &str) -> Result<Config> {
    let mut config = Config {
        doc: Map::new(),
        mode: 0o644,
    };
    let raw = match std::fs::read(file) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(config),
        Err(e) => return Err(e.into()),
    };
    if let Ok(meta) = std::fs::metadata(file) {
        config.mode = meta.permissions().mode() & 0o777;
    }
    if String::from_utf8_lossy(&raw).trim().is_empty() {
        return Ok(config);
    }
    match serde_json::from_slice::<Value>(&raw) {
        Ok(Value::Object(m)) => config.doc = m,
        // A JSON null leaves the document empty.
        Ok(Value::Null) => {}
        Ok(other) => {
            return Err(anyhow!(
                "obsidian config: json: cannot unmarshal {} into Go value of type map[string]interface {{}}",
                json_kind(&other)
            ))
        }
        Err(e) => return Err(anyhow!("obsidian config: {e}")),
    }
    Ok(config)
}

/// The JSON type name used in the "cannot unmarshal" config error.
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

/// True when some vault entry already points at `folder`.
fn is_registered(vaults: &Map<String, Value>, folder: &str) -> bool {
    vaults.values().filter_map(Value::as_object).any(|entry| {
        let existing = entry.get("path").and_then(|p| p.as_str()).unwrap_or("");
        same_path(existing, folder)
    })
}

fn register_vault(env: &dyn ObsidianEnv, folder: &str) -> Result<String> {
    let file = env.config_file();
    if file.trim().is_empty() {
        return Err(anyhow!("Obsidian config path is not set"));
    }
    mkdir_all(&path::dir(&file))?;
    let Config { mut doc, mode } = read_config(&file)?;
    let mut vaults = match doc.remove("vaults") {
        Some(Value::Object(m)) => m,
        Some(Value::Null) | None => Map::new(),
        Some(_) => return Err(anyhow!("obsidian config: vaults is not an object")),
    };
    if is_registered(&vaults, folder) {
        return Ok(format!("Obsidian vault already registered at {folder}"));
    }
    let mut entry = Map::new();
    entry.insert("path".to_string(), Value::String(folder.to_string()));
    entry.insert("ts".to_string(), Value::from(unix_millis()));
    vaults.insert(kiki_util::random_hex(8), Value::Object(entry));
    doc.insert("vaults".to_string(), Value::Object(vaults));
    let mut out = kiki_util::json::marshal_indent(&Value::Object(doc));
    out.push('\n');
    write_atomically(&file, out.as_bytes(), mode)?;
    Ok(format!("Registered Obsidian vault at {folder}"))
}

/// Writes `data` next to `file` and renames it into place.
fn write_atomically(file: &str, data: &[u8], mode: u32) -> Result<()> {
    let tmp = format!("{file}.tmp");
    write_file(&tmp, data, mode)?;
    if let Err(e) = std::fs::rename(&tmp, file) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

fn unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
