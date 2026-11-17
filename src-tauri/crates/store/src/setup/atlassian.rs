//! The Atlassian MCP section of mcp.yaml.

use std::fs;

use anyhow::{anyhow, Result};
use kiki_mcp::Atlassian;
use serde_json::Value;

use kiki_util::path::{dir, join};
use kiki_yaml::{Mapping, Value as YamlValue};

use super::lenient::{parse, top_mapping};
use crate::fsutil;
use crate::store::Store;

impl Store {
    fn mcp_path(&self) -> String {
        join(&[&dir(&self.path), "mcp.yaml"])
    }

    fn legacy_atlassian_path(&self) -> String {
        join(&[&dir(&self.path), "atlassian.yaml"])
    }

    /// Atlassian reads the Jira and Confluence section of mcp.yaml.
    /// A missing file is an empty setup. An older atlassian.yaml is moved into mcp.yaml.
    pub fn atlassian(&self) -> Result<Atlassian> {
        let _guard = self.lock();
        self.read_atlassian()
    }

    fn read_atlassian(&self) -> Result<Atlassian> {
        let raw = match fs::read(self.mcp_path()) {
            Ok(raw) => raw,
            Err(err) => {
                if fsutil::not_exist(&err) {
                    return self.migrate_atlassian_file();
                }
                return Err(err.into());
            }
        };
        let cfg = parse_mcp(&String::from_utf8_lossy(&raw)).map_err(|err| anyhow!("read MCP settings: {err}"))?;
        self.remove_legacy_atlassian()?;
        Ok(cfg.normalized())
    }

    /// migrateAtlassianFile copies a root-level atlassian.yaml into mcp.yaml and removes the old file.
    fn migrate_atlassian_file(&self) -> Result<Atlassian> {
        let raw = match fs::read(self.legacy_atlassian_path()) {
            Ok(raw) => raw,
            Err(err) => {
                if fsutil::not_exist(&err) {
                    return Ok(Atlassian::default());
                }
                return Err(err.into());
            }
        };
        let cfg = parse_atlassian(&String::from_utf8_lossy(&raw))
            .map_err(|err| anyhow!("read Atlassian MCP settings: {err}"))?
            .normalized();
        if cfg.empty() {
            self.remove_legacy_atlassian()?;
            return Ok(Atlassian::default());
        }
        self.write_mcp(&cfg)?;
        self.remove_legacy_atlassian()?;
        Ok(cfg)
    }

    /// SaveAtlassian stores Jira and Confluence inside mcp.yaml.
    /// An empty setup removes the file. Agents read the setup file, not this one.
    pub fn save_atlassian(&self, cfg: &Atlassian) -> Result<()> {
        let _guard = self.lock();
        let cfg = cfg.normalized();
        self.write_mcp(&cfg)?;
        self.remove_legacy_atlassian()
    }

    fn write_mcp(&self, cfg: &Atlassian) -> Result<()> {
        let path = self.mcp_path();
        if cfg.empty() {
            return Ok(fsutil::remove_if_exists(&path)?);
        }
        cfg.validate()?;
        let section = atlassian_node(cfg)?;
        let text = format!(
            "# MCP credentials for this computer. Agents do not read this file.\n{}",
            Mapping::new().set("atlassian", section).emit()
        );
        let tmp = format!("{path}.tmp");
        fsutil::write_file(&tmp, text.as_bytes(), 0o600)?;
        if let Err(err) = fsutil::chmod(&tmp, 0o600) {
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        if let Err(err) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        Ok(fsutil::chmod(&path, 0o600)?)
    }

    fn remove_legacy_atlassian(&self) -> Result<()> {
        let legacy = self.legacy_atlassian_path();
        for path in [legacy.clone(), format!("{legacy}.tmp")] {
            fsutil::remove_if_exists(&path)?;
        }
        Ok(())
    }
}

/// Decodes `mcp.yaml`: the `atlassian` section, or the zero setup when it is missing or null.
fn parse_mcp(raw: &str) -> Result<Atlassian> {
    let value = parse(raw)?;
    let map = top_mapping(&value)?;
    match map.get("atlassian") {
        None => Ok(Atlassian::default()),
        Some(section) => Ok(serde_yaml::from_value(section.clone())?),
    }
}

/// Decodes a bare Atlassian setup (the older atlassian.yaml). An empty file is the zero setup.
fn parse_atlassian(raw: &str) -> Result<Atlassian> {
    let value = parse(raw)?;
    Ok(serde_yaml::from_value(value)?)
}

/// Key order of the settings structs: Atlassian (enabled, username, api_token, jira, confluence)
/// and Service (enabled, url, own_credentials, username, api_token, board_id, ...). The two
/// orders agree on every pair of keys they share.
const KEY_ORDER: [&str; 14] = [
    "enabled",
    "url",
    "own_credentials",
    "username",
    "api_token",
    "board_id",
    "board_name",
    "space_id",
    "space_key",
    "space_name",
    "parent_id",
    "parent_title",
    "jira",
    "confluence",
];

/// Builds the YAML node from the serde form of the setup (which already leaves out the empty
/// fields that are omitted).
fn atlassian_node(cfg: &Atlassian) -> Result<YamlValue> {
    Ok(value_node(&serde_json::to_value(cfg)?))
}

fn value_node(value: &Value) -> YamlValue {
    match value {
        Value::Bool(b) => YamlValue::Bool(*b),
        Value::String(s) => YamlValue::Str(s.clone()),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by_key(|key| {
                KEY_ORDER
                    .iter()
                    .position(|k| *k == key.as_str())
                    .unwrap_or(KEY_ORDER.len())
            });
            YamlValue::Map(
                keys.into_iter()
                    .map(|key| (key.clone(), value_node(&map[key])))
                    .collect(),
            )
        }
        Value::Array(items) => YamlValue::Seq(items.iter().map(value_node).collect()),
        other => YamlValue::Str(other.to_string()),
    }
}
