//! The setup passed to one agent launch: repositories, their folders, and the saved options.

use std::collections::BTreeMap;
use std::fs;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use kiki_toolchain as toolchain;
use kiki_util::path;

/// One app repository. `index` is obsidian, codegraph, or empty when none is set.
/// `sparse` is a folder inside the git repository. Empty means the whole repository is checked out.
///
/// The JSON keys are capitalised (`Kind`, `Folder`, ...) because setup data already stored uses them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Repo {
    #[serde(rename = "Kind")]
    pub kind: String,
    #[serde(rename = "Folder")]
    pub folder: String,
    #[serde(rename = "Services")]
    pub services: Vec<String>,
    #[serde(rename = "Index")]
    pub index: String,
    #[serde(rename = "Sparse")]
    pub sparse: String,
}

/// The setup passed to one agent launch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    #[serde(rename = "Root")]
    pub root: String,
    #[serde(rename = "SetupFile")]
    pub setup_file: String,
    #[serde(rename = "Worktree")]
    pub worktree: String,
    #[serde(rename = "WorkspaceID")]
    pub workspace_id: String,
    #[serde(rename = "Orchestrator")]
    pub orchestrator: String,
    #[serde(rename = "AgentPath")]
    pub agent_path: String,
    #[serde(rename = "GenerateTool")]
    pub generate_tool: String,
    #[serde(rename = "GenerateModel")]
    pub generate_model: String,
    #[serde(rename = "Repos")]
    pub repos: Vec<Repo>,
}

impl Snapshot {
    /// The setup for a folder kind.
    pub fn repo(&self, kind: &str) -> Repo {
        match self.repos.iter().find(|repo| repo.kind == kind) {
            Some(repo) => repo.clone(),
            None => Repo {
                kind: kind.to_string(),
                ..Default::default()
            },
        }
    }
}

/// Checks a folder path for a sparse checkout.
/// An empty path means the whole repository. The result uses forward slashes.
pub fn sparse_path(raw: &str) -> Result<String> {
    let raw = raw.trim();
    let raw = raw.trim_matches('/');
    if raw.is_empty() {
        return Ok(String::new());
    }
    const MSG: &str = "folder path must be inside the repository, such as product/requirements";
    if raw.contains(['\\', '\r', '\n']) || raw.contains(':') {
        return Err(anyhow!(MSG));
    }
    let cleaned = path::clean(raw);
    if cleaned == "." || cleaned == ".." || cleaned.starts_with("../") {
        return Err(anyhow!(MSG));
    }
    for part in cleaned.split('/') {
        if part.is_empty() || part == "." || part == ".." || part == ".git" {
            return Err(anyhow!(MSG));
        }
    }
    Ok(cleaned)
}

/// Gives every repository a folder and creates any folder that is missing.
/// A saved absolute path stays. An empty path uses the directory under the app root.
/// Saved service names are kept.
pub fn apply_root(root: &str, mut snap: Snapshot) -> Result<Snapshot> {
    let root = root.trim();
    if root.is_empty() || !path::is_abs(root) {
        return Err(anyhow!("app root is not set"));
    }
    let root = path::clean(root);
    let by_kind: BTreeMap<String, Repo> = snap
        .repos
        .iter()
        .map(|repo| (repo.kind.clone(), repo.clone()))
        .collect();
    snap.root = root.clone();
    snap.repos = Vec::with_capacity(toolchain::KINDS.len());
    for kind in toolchain::KINDS {
        let mut repo = by_kind.get(*kind).cloned().unwrap_or_default();
        repo.kind = kind.to_string();
        repo.folder = repo_folder(&root, kind, &repo.folder);
        if *kind == "document" {
            repo.services = Vec::new();
        }
        fs::create_dir_all(&repo.folder)?;
        snap.repos.push(repo);
    }
    // The extra repositories follow the fixed ones, sorted by kind.
    for (kind, saved) in &by_kind {
        if toolchain::known(kind) || !toolchain::repo_id(kind) {
            continue;
        }
        let mut repo = saved.clone();
        repo.folder = repo_folder(&root, kind, &repo.folder);
        fs::create_dir_all(&repo.folder)?;
        snap.repos.push(repo);
    }
    Ok(snap)
}

fn repo_folder(root: &str, kind: &str, saved: &str) -> String {
    let saved = saved.trim();
    if !saved.is_empty() && path::is_abs(saved) {
        return path::clean(saved);
    }
    if toolchain::known(kind) {
        return toolchain::folder(root, kind);
    }
    path::join(&[root, kind])
}

/// Cleans the saved folder paths.
pub fn fill(mut snap: Snapshot) -> Snapshot {
    for repo in snap.repos.iter_mut() {
        let folder = repo.folder.trim();
        repo.folder = if folder.is_empty() {
            String::new()
        } else {
            path::clean(folder)
        };
    }
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_with_folder(kind: &str, folder: &str) -> Repo {
        Repo {
            kind: kind.into(),
            folder: folder.into(),
            ..Default::default()
        }
    }

    #[test]
    fn apply_root_keeps_an_absolute_folder() {
        let root = tempfile::tempdir().unwrap();
        let custom = tempfile::tempdir().unwrap();
        let root = root.path().to_string_lossy().into_owned();
        let custom = custom.path().to_string_lossy().into_owned();
        let snap = apply_root(
            &root,
            Snapshot {
                repos: vec![
                    repo_with_folder("backend", &custom),
                    repo_with_folder("website", "relative"),
                ],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(snap.repo("backend").folder, custom);
        assert_eq!(snap.repo("website").folder, path::join(&[&root, "website"]));
        assert_eq!(snap.repo("document").folder, path::join(&[&root, "docs"]));
    }

    #[test]
    fn apply_root_needs_an_absolute_root() {
        for root in ["", "  ", "relative/root"] {
            let err = apply_root(root, Snapshot::default()).unwrap_err();
            assert_eq!(err.to_string(), "app root is not set", "root {root:?}");
        }
    }

    #[test]
    fn apply_root_appends_extra_repositories_in_sorted_order() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().to_string_lossy().into_owned();
        let snap = apply_root(
            &root,
            Snapshot {
                repos: vec![
                    repo_with_folder("zeta", ""),
                    repo_with_folder("alpha", ""),
                    repo_with_folder("Bad Id", ""),
                ],
                ..Default::default()
            },
        )
        .unwrap();
        let kinds: Vec<&str> = snap.repos.iter().map(|repo| repo.kind.as_str()).collect();
        assert_eq!(kinds, ["backend", "website", "mobile", "document", "alpha", "zeta"]);
        assert_eq!(snap.repo("alpha").folder, path::join(&[&root, "alpha"]));
    }

    #[test]
    fn fill_cleans_folders() {
        let snap = fill(Snapshot {
            repos: vec![
                repo_with_folder("backend", " /a/b/../c/ "),
                repo_with_folder("website", "  "),
            ],
            ..Default::default()
        });
        assert_eq!(snap.repos[0].folder, "/a/c");
        assert_eq!(snap.repos[1].folder, "");
    }

    #[test]
    fn sparse_path_cases() {
        assert_eq!(sparse_path(" /product/kiki/ ").unwrap(), "product/kiki");
        assert_eq!(sparse_path("/product/kiki").unwrap(), "product/kiki");
        assert_eq!(sparse_path("").unwrap(), "");
        for raw in [
            "../secret",
            "product/../../etc",
            "product/.git",
            r"product\kiki",
            "foo:bar",
        ] {
            assert!(sparse_path(raw).is_err(), "accepted {raw:?}");
        }
    }
}
