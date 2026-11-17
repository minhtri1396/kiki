//! The shared form of a repository: `repos/<id>.yaml` in a team folder holds only what is the same on every
//! machine (services, index tool, sparse folder), never the local checkout folder.
//!
//! Pattern: Builder. [`SharedRepo::canonical`] assembles the file with `kiki_yaml::Mapping`.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{anyhow, Result};
use kiki_console as console;
use kiki_toolchain as toolchain;
use kiki_util::path;
use kiki_yaml::{Mapping, Value};
use serde::Deserialize;

use super::files::{is_real_dir, write_shared_file, TempDir};
use kiki_teamfolder as teamfolder;

/// A repository as shared through the team folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SharedRepo {
    pub id: String,
    pub services: Vec<String>,
    pub index: String,
    pub sparse: String,
}

impl SharedRepo {
    /// The YAML text of the shared fields. It is what is written to the team folder and what is hashed and
    /// diffed, so two folders compare equal exactly when these bytes are equal.
    pub(crate) fn canonical(&self) -> String {
        let services = self
            .services
            .iter()
            .map(|s| Value::from(s.as_str()))
            .collect::<Vec<_>>();
        Mapping::new()
            .set("id", self.id.as_str())
            .set("services", services)
            .set_unless_empty("index", &self.index)
            .set_unless_empty("sparse", &self.sparse)
            .emit()
    }

    /// Copies the shared fields onto a local repository, keeping its checkout folder.
    pub(crate) fn apply_to(&self, mut repo: console::Repo) -> console::Repo {
        repo.kind = self.id.clone();
        repo.services = if self.id == "document" {
            Vec::new()
        } else {
            self.services.clone()
        };
        repo.index = self.index.clone();
        repo.sparse = self.sparse.clone();
        repo
    }
}

/// Parses `<id>.yaml`. `Ok(None)` means the file name is not a repository id.
pub(crate) fn parse_shared_repo(raw: &[u8], file_name: &str) -> Result<Option<SharedRepo>> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Row {
        services: Option<Vec<String>>,
        index: Option<String>,
        sparse: Option<String>,
    }
    let text = String::from_utf8_lossy(raw);
    // An empty document is the zero value, not an error.
    let row: Row = if text.trim().is_empty() {
        Row::default()
    } else {
        match serde_yaml::from_str::<Option<Row>>(&text) {
            Ok(row) => row.unwrap_or_default(),
            Err(e) => return Err(anyhow!("read {file_name}: {e}")),
        }
    };
    let id = file_name.strip_suffix(".yaml").unwrap_or(file_name).to_string();
    if !toolchain::repo_id(&id) {
        return Ok(None);
    }
    let mut services: Vec<String> = row
        .services
        .unwrap_or_default()
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if id == "document" {
        services = Vec::new();
    }
    let mut index = row.index.unwrap_or_default().trim().to_string();
    if index != "obsidian" && index != "codegraph" {
        index = String::new();
    }
    let sparse = console::sparse_path(&row.sparse.unwrap_or_default()).map_err(|e| anyhow!("{id}: {e}"))?;
    Ok(Some(SharedRepo {
        id,
        services,
        index,
        sparse,
    }))
}

/// Every repository file in `dir`, sorted by id. A missing or non-directory `dir` has none.
pub(crate) fn list_shared_repos(dir: &str) -> Result<Vec<SharedRepo>> {
    if !is_real_dir(dir) {
        return Ok(Vec::new());
    }
    let mut names: Vec<(String, bool)> = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| anyhow!("open {dir}: {e}"))? {
        let entry = entry.map_err(|e| anyhow!("readdirent {dir}: {e}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // `file_type` does not follow symlinks.
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        names.push((name, is_dir));
    }
    names.sort();
    let mut repos = Vec::with_capacity(names.len());
    for (name, is_dir) in names {
        if is_dir || !name.ends_with(".yaml") || name.starts_with('.') {
            continue;
        }
        let file = path::join(&[dir, &name]);
        let raw = std::fs::read(&file).map_err(|e| anyhow!("open {file}: {e}"))?;
        if let Some(repo) = parse_shared_repo(&raw, &name)? {
            repos.push(repo);
        }
    }
    repos.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(repos)
}

/// Writes this app's shared repositories to `dest` (a member's `repos` folder), replacing what is there.
pub(crate) fn publish_repos(src: &str, dest: &str) -> Result<()> {
    let repos = list_shared_repos(src)?;
    if repos.is_empty() {
        return Err(anyhow!("This app has no repository files to copy."));
    }
    let staging = TempDir::create()?;
    for repo in &repos {
        let file = path::join(&[staging.path(), &format!("{}.yaml", repo.id)]);
        write_shared_file(&file, repo.canonical().as_bytes())?;
    }
    teamfolder::copy(staging.path(), dest)
}

/// The repository list after adopting `shared`: the built-in kinds first (in their fixed order), then the extra
/// ids sorted. Local checkout folders are kept; an extra repository this app did not have starts without one.
/// Also returns the ids that were added.
pub(crate) fn merge_shared(local: &[console::Repo], shared: Vec<SharedRepo>) -> (Vec<console::Repo>, Vec<String>) {
    let mut by_id: BTreeMap<String, SharedRepo> = shared.into_iter().map(|repo| (repo.id.clone(), repo)).collect();
    let known: HashMap<&str, &console::Repo> = local.iter().map(|repo| (repo.kind.as_str(), repo)).collect();
    let mut next = Vec::with_capacity(toolchain::KINDS.len() + by_id.len());
    for kind in toolchain::KINDS {
        let mut repo = known.get(*kind).map(|r| (*r).clone()).unwrap_or_default();
        repo.kind = kind.to_string();
        if let Some(file) = by_id.remove(*kind) {
            repo = file.apply_to(repo);
        }
        next.push(repo);
    }
    let mut added = Vec::new();
    for (id, file) in by_id {
        let previous = known.get(id.as_str()).map(|r| (*r).clone());
        let existed = previous.is_some();
        let mut repo = previous.unwrap_or_default();
        repo.kind = id.clone();
        repo = file.apply_to(repo);
        if !existed {
            repo.folder = String::new();
            added.push(id);
        }
        next.push(repo);
    }
    (next, added)
}

/// Deletes `<id>.yaml` files in `dir` whose id is not in `keep`.
pub(crate) fn remove_extra_repo_files(dir: &str, keep: &HashSet<&str>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(anyhow!("open {dir}: {e}")),
    };
    for entry in entries {
        let entry = entry.map_err(|e| anyhow!("readdirent {dir}: {e}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir || !name.ends_with(".yaml") {
            continue;
        }
        let id = name.strip_suffix(".yaml").unwrap_or(&name);
        if keep.contains(id) {
            continue;
        }
        let file = path::join(&[dir, &name]);
        std::fs::remove_file(&file).map_err(|e| anyhow!("remove {file}: {e}"))?;
    }
    Ok(())
}
