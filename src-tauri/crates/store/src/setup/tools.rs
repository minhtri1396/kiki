//! The tools turned on for this computer, in tools.yaml.

use std::fs;

use anyhow::{anyhow, Result};
use kiki_tools::Settings;

use kiki_util::path::{dir, join};
use kiki_yaml::Mapping;

use super::lenient::{get_bool, read_map};
use crate::fsutil;
use crate::store::Store;

impl Store {
    fn tools_path(&self) -> String {
        join(&[&dir(&self.path), "tools.yaml"])
    }

    /// Tools reads the tools turned on for this computer.
    /// A missing file means none are turned on.
    pub fn tools(&self) -> Result<Settings> {
        let _guard = self.lock();
        self.read_tools()
    }

    fn read_tools(&self) -> Result<Settings> {
        let raw = match fs::read(self.tools_path()) {
            Ok(raw) => raw,
            Err(err) => {
                if fsutil::not_exist(&err) {
                    return Ok(Settings::default());
                }
                return Err(err.into());
            }
        };
        parse_tools(&String::from_utf8_lossy(&raw)).map_err(|err| anyhow!("read tool settings: {err}"))
    }

    /// SaveTools stores which tools are turned on.
    /// Turning every tool off removes the file. Agents read the setup file, not this one.
    pub fn save_tools(&self, cfg: &Settings) -> Result<()> {
        let _guard = self.lock();
        self.write_tools(cfg)
    }

    fn write_tools(&self, cfg: &Settings) -> Result<()> {
        let path = self.tools_path();
        if cfg.empty() {
            return Ok(fsutil::remove_if_exists(&path)?);
        }
        // Like `omitempty`: false flags are left out; a guardrails flag is written
        // whenever it is set, even to false.
        let mut entries = Mapping::new()
            .set_if(cfg.github_cli, "github_cli", true)
            .set_if(cfg.rtk, "rtk", true);
        if let Some(on) = cfg.guardrails {
            entries = entries.set("guardrails", on);
        }
        let text = format!(
            "# Tools turned on for this computer. Agents do not read this file.\n{}",
            entries.emit()
        );
        let tmp = format!("{path}.tmp");
        fsutil::write_file(&tmp, text.as_bytes(), 0o644)?;
        if let Err(err) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        Ok(())
    }
}

fn parse_tools(raw: &str) -> Result<Settings> {
    let map = read_map(raw)?;
    Ok(Settings {
        github_cli: get_bool(&map, "github_cli")?.unwrap_or(false),
        rtk: get_bool(&map, "rtk")?.unwrap_or(false),
        guardrails: get_bool(&map, "guardrails")?,
    })
}
