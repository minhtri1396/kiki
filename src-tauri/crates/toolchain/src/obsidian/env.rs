//! Pattern: Dependency Injection. `ObsidianEnv` is everything the Obsidian helpers read from the machine
//! (vault list, app binaries, link directory, relaunch). `SystemEnv` is the production default; tests pass their own.

use anyhow::{anyhow, Result};
use kiki_util::path;
use std::process::Command;
use std::time::{Duration, Instant};

/// The machine-specific facts and actions behind the Obsidian helpers.
pub trait ObsidianEnv: Send + Sync {
    /// The Obsidian vault list (`obsidian.json`); empty when it cannot be located.
    fn config_file(&self) -> String;
    /// Quits Obsidian and opens `folder` in it.
    fn relaunch(&self, folder: &str) -> Result<()>;
    /// Where the Obsidian app may keep its bundled CLI, in order of preference.
    fn cli_candidates(&self) -> Vec<String>;
    /// The directory where Set up places an `obsidian` command; empty when there is none.
    fn link_dir(&self) -> String;
}

/// The real machine: platform paths from the environment and the `osascript`/`open` relaunch.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemEnv;

fn home_dir() -> Option<String> {
    std::env::var("HOME").ok().filter(|h| !h.is_empty())
}

fn env_dir(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl ObsidianEnv for SystemEnv {
    fn config_file(&self) -> String {
        let Some(home) = home_dir() else { return String::new() };
        if cfg!(target_os = "macos") {
            path::join(&[&home, "Library", "Application Support", "obsidian", "obsidian.json"])
        } else if cfg!(target_os = "windows") {
            match env_dir("APPDATA") {
                Some(app) => path::join(&[&app, "obsidian", "obsidian.json"]),
                None => path::join(&[&home, "AppData", "Roaming", "obsidian", "obsidian.json"]),
            }
        } else {
            match env_dir("XDG_CONFIG_HOME") {
                Some(xdg) => path::join(&[&xdg, "obsidian", "obsidian.json"]),
                None => path::join(&[&home, ".config", "obsidian", "obsidian.json"]),
            }
        }
    }

    fn relaunch(&self, folder: &str) -> Result<()> {
        if !cfg!(target_os = "macos") {
            return Err(anyhow!("open {folder} in Obsidian"));
        }
        let _ = Command::new("osascript")
            .args(["-e", r#"tell application "Obsidian" to quit"#])
            .status();
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            let running = Command::new("pgrep")
                .args(["-x", "Obsidian"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !running {
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        let status = Command::new("open").args(["-a", "Obsidian", folder]).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("{status}"))
        }
    }

    fn cli_candidates(&self) -> Vec<String> {
        let home = home_dir().unwrap_or_default();
        if cfg!(target_os = "macos") {
            vec![
                "/Applications/Obsidian.app/Contents/MacOS/obsidian-cli".to_string(),
                path::join(&[
                    &home,
                    "Applications",
                    "Obsidian.app",
                    "Contents",
                    "MacOS",
                    "obsidian-cli",
                ]),
            ]
        } else if cfg!(target_os = "windows") {
            let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
            let program_files = std::env::var("ProgramFiles").unwrap_or_default();
            vec![
                path::join(&[&local, "Programs", "Obsidian", "obsidian-cli.exe"]),
                path::join(&[&program_files, "Obsidian", "obsidian-cli.exe"]),
            ]
        } else {
            vec![
                "/usr/local/bin/obsidian-cli".to_string(),
                "/usr/bin/obsidian-cli".to_string(),
                path::join(&[&home, ".local", "share", "obsidian", "obsidian-cli"]),
            ]
        }
    }

    fn link_dir(&self) -> String {
        match home_dir() {
            Some(home) => path::join(&[&home, ".local", "bin"]),
            None => String::new(),
        }
    }
}
