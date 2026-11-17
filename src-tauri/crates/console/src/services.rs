//! Finding the services (Nx projects) of a code repository.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::Deserialize;

use kiki_util::path;

/// A service name: letters, digits, dots, hyphens, and underscores.
pub(crate) static SERVICE_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$").unwrap());

/// The most of a `project.json` that is read.
const MAX_PROJECT_JSON_BYTES: u64 = 256 * 1024;

/// Lists Nx project names from project.json files in a repository folder.
/// An empty folder returns no names. node_modules and other tool folders are skipped.
pub fn scan_services(folder: &str) -> Result<Vec<String>> {
    let folder = folder.trim();
    if folder.is_empty() {
        return Ok(Vec::new());
    }
    if folder.contains(['\r', '\n']) || !path::is_abs(folder) {
        return Err(anyhow!("folder must be an absolute path"));
    }
    match fs::metadata(folder) {
        Ok(info) if info.is_dir() => {}
        _ => return Err(anyhow!("folder does not exist")),
    }
    let mut found: BTreeSet<String> = BTreeSet::new();
    // A symlinked root is not walked: it is not a directory itself.
    let root_is_dir = fs::symlink_metadata(folder).map(|m| m.is_dir()).unwrap_or(false);
    if root_is_dir {
        walk_services(Path::new(folder), &mut found);
    }
    Ok(found.into_iter().collect())
}

/// Walks a directory in name order, ignoring unreadable entries.
fn walk_services(dir: &Path, found: &mut BTreeSet<String>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<(String, PathBuf, bool)> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        entries.push((name, entry.path(), is_dir));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, path, is_dir) in entries {
        if is_dir {
            if !skip_service_dir(&name) {
                walk_services(&path, found);
            }
        } else if name == "project.json" {
            if let Some(name) = read_project_name(&path).filter(|name| SERVICE_NAME.is_match(name)) {
                found.insert(name);
            }
        }
    }
}

fn skip_service_dir(name: &str) -> bool {
    match name {
        "node_modules" | "dist" | "coverage" | "vendor" | "tmp" => true,
        _ => name.starts_with('.'),
    }
}

fn read_project_name(path: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct Doc {
        #[serde(default)]
        name: Option<String>,
    }
    let file = fs::File::open(path).ok()?;
    let limited = file.take(MAX_PROJECT_JSON_BYTES);
    let mut de = serde_json::Deserializer::from_reader(limited);
    let doc = Doc::deserialize(&mut de).ok()?;
    Some(doc.name.unwrap_or_default().trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, text: &str) {
        let file = root.join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

    #[test]
    fn scan_finds_names_and_skips_tool_folders() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "apps/api/project.json", r#"{"name":"api"}"#);
        write(dir.path(), "libs/shared/project.json", r#"{"name":" shared "}"#);
        write(dir.path(), "node_modules/x/project.json", r#"{"name":"hidden"}"#);
        write(dir.path(), ".cache/project.json", r#"{"name":"dot"}"#);
        write(dir.path(), "apps/bad/project.json", r#"{"name":"bad name"}"#);
        write(dir.path(), "apps/broken/project.json", "not json");
        write(dir.path(), "apps/nameless/project.json", "{}");
        let names = scan_services(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(names, ["api", "shared"]);
    }

    #[test]
    fn scan_rejects_bad_folders() {
        assert_eq!(scan_services("  ").unwrap(), Vec::<String>::new());
        assert_eq!(
            scan_services("relative").unwrap_err().to_string(),
            "folder must be an absolute path"
        );
        assert_eq!(
            scan_services("/kiki-no-such-folder").unwrap_err().to_string(),
            "folder does not exist"
        );
    }
}
