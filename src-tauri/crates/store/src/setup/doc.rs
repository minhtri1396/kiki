//! The setup file (`kiki.yaml`): the parsed document, reading it with defaults, and writing it back.

use std::collections::{HashMap, HashSet};
use std::fs;

use anyhow::{anyhow, Result};
use kiki_console as console;
use kiki_toolchain as toolchain;
use kiki_yaml::{Mapping, Value};
use serde::{Deserialize, Serialize};

use super::lenient::{get_str, get_strings, read_map, rows, top_mapping};
use crate::fsutil;

/// TeamFolder is a local folder the user syncs between computers.
/// Path is the team folder. User is this person's folder inside it.
/// Member is the member folder last copied into the local agents folder.
///
/// Only the YAML keys are lower case; the JSON keys are the capitalised field names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamFolder {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "User")]
    pub user: String,
    #[serde(rename = "Member")]
    pub member: String,
}

impl TeamFolder {
    fn is_zero(&self) -> bool {
        self.path.is_empty() && self.user.is_empty() && self.member.is_empty()
    }

    pub(crate) fn trimmed(&self) -> TeamFolder {
        TeamFolder {
            path: self.path.trim().to_string(),
            user: self.user.trim().to_string(),
            member: self.member.trim().to_string(),
        }
    }
}

/// The parsed setup file.
#[derive(Debug, Clone, Default)]
pub(crate) struct FileDoc {
    pub workspace_id: String,
    pub orchestrator: String,
    pub agent_path: String,
    pub generate_tool: String,
    pub generate_model: String,
    pub repos: Vec<RepoDoc>,
    pub team_folder: TeamFolder,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RepoDoc {
    pub kind: String,
    pub folder: String,
    pub services: Vec<String>,
}

/// Decodes the setup text into a document (no defaults applied).
pub(crate) fn parse_doc(raw: &str) -> Result<FileDoc> {
    parse_doc_inner(raw).map_err(|err| anyhow!("read setup file: {err}"))
}

fn parse_doc_inner(raw: &str) -> Result<FileDoc> {
    let map = read_map(raw)?;
    let mut doc = FileDoc {
        workspace_id: get_str(&map, "workspace_id"),
        orchestrator: get_str(&map, "orchestrator"),
        agent_path: get_str(&map, "agent_path"),
        generate_tool: get_str(&map, "generate_tool"),
        generate_model: get_str(&map, "generate_model"),
        ..Default::default()
    };
    for row in rows(&map, "repos", "repoDoc")? {
        doc.repos.push(RepoDoc {
            kind: get_str(&row, "kind"),
            folder: get_str(&row, "folder"),
            services: get_strings(&row, "services")?.unwrap_or_default(),
        });
    }
    if let Some(team) = map.get("team_folder") {
        let team = top_mapping(team)?;
        doc.team_folder = TeamFolder {
            path: get_str(&team, "path"),
            user: get_str(&team, "user"),
            member: get_str(&team, "member"),
        };
    }
    Ok(doc)
}

/// Reads the setup file and applies the defaults. A file that still has `orca_agent`
/// is rewritten without it.
pub(crate) fn read_doc(path: &str) -> Result<FileDoc> {
    let raw = fs::read(path)?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let mut doc = parse_doc(&text)?;
    if doc.workspace_id.is_empty() {
        doc.workspace_id = "kiki".to_string();
    }
    if doc.orchestrator.trim().is_empty() {
        doc.orchestrator = "orca".to_string();
    }
    doc.orchestrator = doc.orchestrator.trim().to_string();
    doc.repos = normalize_repos(&doc.repos);
    if yaml_has_key(&raw, "orca_agent") {
        write_doc(path, &doc)?;
    }
    Ok(doc)
}

fn yaml_has_key(raw: &[u8], key: &str) -> bool {
    let prefix = format!("{key}:");
    for line in String::from_utf8_lossy(raw).split('\n') {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with(&prefix) {
            return true;
        }
    }
    false
}

/// The YAML text of a document, with the header comment the file always starts with.
fn doc_text(doc: &FileDoc) -> String {
    let repos = normalize_repos(&doc.repos);
    let mut map = Mapping::new().set("workspace_id", doc.workspace_id.as_str());
    for (key, value) in [
        ("orchestrator", &doc.orchestrator),
        ("agent_path", &doc.agent_path),
        ("generate_tool", &doc.generate_tool),
        ("generate_model", &doc.generate_model),
    ] {
        map = map.set_unless_empty(key, value);
    }
    let repo_rows = repos
        .iter()
        .map(|repo| {
            let mut row = Mapping::new()
                .set("kind", repo.kind.as_str())
                .set_unless_empty("folder", &repo.folder);
            if !repo.services.is_empty() {
                row = row.set(
                    "services",
                    Value::Seq(repo.services.iter().map(|s| Value::from(s.as_str())).collect()),
                );
            }
            row.into_value()
        })
        .collect();
    map = map.set("repos", Value::Seq(repo_rows));
    if !doc.team_folder.is_zero() {
        let team = Mapping::new()
            .set_unless_empty("path", &doc.team_folder.path)
            .set_unless_empty("user", &doc.team_folder.user)
            .set_unless_empty("member", &doc.team_folder.member);
        map = map.set("team_folder", team);
    }
    format!(
        "# KIKI setup. This file is saved in the .kiki folder next to the app. Agents read it.\n{}",
        map.emit()
    )
}

/// Writes the setup file through a `.tmp` file and a rename.
pub(crate) fn write_doc(path: &str, doc: &FileDoc) -> Result<()> {
    fsutil::write_atomic(path, doc_text(doc).as_bytes(), 0o644)?;
    Ok(())
}

pub(crate) fn default_doc() -> FileDoc {
    let seeded: Vec<RepoDoc> = toolchain::KINDS
        .iter()
        .map(|kind| RepoDoc {
            kind: (*kind).to_string(),
            ..Default::default()
        })
        .collect();
    FileDoc {
        workspace_id: "kiki".to_string(),
        orchestrator: "orca".to_string(),
        repos: normalize_repos(&seeded),
        ..Default::default()
    }
}

/// The built-in kinds that are present, in their fixed order, then any other valid kinds sorted by name.
/// A kind that is missing stays missing, so a deleted repository is not written back.
pub(crate) fn normalize_repos(repos: &[RepoDoc]) -> Vec<RepoDoc> {
    let mut by_kind: HashMap<&str, &RepoDoc> = HashMap::new();
    for repo in repos {
        by_kind.insert(repo.kind.as_str(), repo);
    }
    let mut out: Vec<RepoDoc> = Vec::new();
    for kind in toolchain::KINDS {
        let Some(saved) = by_kind.get(kind) else { continue };
        let mut repo = (*saved).clone();
        repo.kind = kind.to_string();
        out.push(repo);
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut extras: Vec<RepoDoc> = Vec::new();
    for repo in repos {
        let kind = repo.kind.trim().to_string();
        if toolchain::known(&kind) || !toolchain::repo_id(&kind) || seen.contains(&kind) {
            continue;
        }
        seen.insert(kind.clone());
        let mut repo = repo.clone();
        repo.kind = kind;
        extras.push(repo);
    }
    extras.sort_by(|a, b| a.kind.cmp(&b.kind));
    out.extend(extras);
    out
}

pub(crate) fn repos_equal(a: &[RepoDoc], b: &[RepoDoc]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.kind == y.kind && x.folder == y.folder && x.services == y.services)
}

pub(crate) fn repos_from(snap: &console::Snapshot) -> Vec<RepoDoc> {
    let out: Vec<RepoDoc> = snap
        .repos
        .iter()
        .map(|repo| RepoDoc {
            kind: repo.kind.clone(),
            folder: repo.folder.clone(),
            services: repo.services.clone(),
        })
        .collect();
    normalize_repos(&out)
}

impl FileDoc {
    pub(crate) fn snapshot(&self) -> console::Snapshot {
        console::Snapshot {
            workspace_id: self.workspace_id.clone(),
            orchestrator: self.orchestrator.clone(),
            agent_path: self.agent_path.clone(),
            generate_tool: self.generate_tool.trim().to_string(),
            generate_model: self.generate_model.trim().to_string(),
            repos: self
                .repos
                .iter()
                .map(|repo| console::Repo {
                    kind: repo.kind.clone(),
                    folder: repo.folder.clone(),
                    services: repo.services.clone(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }
}
