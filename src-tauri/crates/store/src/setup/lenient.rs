//! Pattern: Adapter. Lenient readers over `serde_yaml::Value` that decode the setup files like `yaml.v3`
//! decodes them into structs: null is the zero value, numbers and booleans read as their text, unknown keys
//! are ignored. This reader stays on `serde_yaml` because the MCP settings are decoded with `serde` from the
//! same value; the files are written through `kiki_yaml`.

use anyhow::{anyhow, Result};
use serde_yaml::{Mapping, Value};

/// Parses YAML text. An empty document is null, as `yaml.Unmarshal` leaves the value untouched.
pub(crate) fn parse(raw: &str) -> std::result::Result<Value, serde_yaml::Error> {
    if raw.lines().all(|line| {
        let line = line.trim();
        line.is_empty() || line.starts_with('#')
    }) {
        return Ok(Value::Null);
    }
    serde_yaml::from_str(raw)
}

/// The top-level mapping of a document. Null is an empty mapping; any other type is an error.
pub(crate) fn top_mapping(value: &Value) -> Result<Mapping> {
    match value {
        Value::Null => Ok(Mapping::new()),
        Value::Mapping(m) => Ok(m.clone()),
        other => Err(anyhow!(
            "yaml: unmarshal errors:\n  cannot unmarshal {} into a mapping",
            kind(other)
        )),
    }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "!!null",
        Value::Bool(_) => "!!bool",
        Value::Number(_) => "!!number",
        Value::String(_) => "!!str",
        Value::Sequence(_) => "!!seq",
        Value::Mapping(_) => "!!map",
        Value::Tagged(_) => "!!tagged",
    }
}

/// A string field: null and a missing key are empty; numbers and booleans keep their text.
pub(crate) fn get_str(map: &Mapping, key: &str) -> String {
    match map.get(key) {
        Some(value) => to_string(value),
        None => String::new(),
    }
}

pub(crate) fn to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// A `[]string` field. `None` is a nil slice (missing or null); an empty list is `Some(vec![])`.
pub(crate) fn get_strings(map: &Mapping, key: &str) -> Result<Option<Vec<String>>> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Sequence(items)) => Ok(Some(items.iter().map(to_string).collect())),
        Some(other) => Err(anyhow!(
            "yaml: unmarshal errors:\n  cannot unmarshal {} into []string",
            kind(other)
        )),
    }
}

/// A bool field. yaml.v3 also reads yes/no/on/off into a bool.
pub(crate) fn get_bool(map: &Mapping, key: &str) -> Result<Option<bool>> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(Value::String(s)) => match s.as_str() {
            "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" => Ok(Some(true)),
            "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF" => Ok(Some(false)),
            _ => Err(anyhow!(
                "yaml: unmarshal errors:\n  cannot unmarshal !!str `{s}` into bool"
            )),
        },
        Some(other) => Err(anyhow!(
            "yaml: unmarshal errors:\n  cannot unmarshal {} into bool",
            kind(other)
        )),
    }
}

/// An integer field (launch ids).
pub(crate) fn get_i64(map: &Mapping, key: &str) -> i64 {
    match map.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        _ => 0,
    }
}

/// Parses `raw` and returns its top-level mapping (empty for an empty document).
pub(crate) fn read_map(raw: &str) -> Result<Mapping> {
    top_mapping(&parse(raw)?)
}

/// The list of mappings under `key`. A missing key or null is an empty list; anything but a list is an error
/// that names the element type, as in `[]repoDoc`.
pub(crate) fn rows(map: &Mapping, key: &str, element: &str) -> Result<Vec<Mapping>> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Sequence(items)) => items.iter().map(top_mapping).collect(),
        Some(_) => Err(anyhow!("yaml: unmarshal errors:\n  cannot unmarshal into []{element}")),
    }
}
