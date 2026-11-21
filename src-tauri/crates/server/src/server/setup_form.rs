//! The Settings form (`PUT /api/setup`) and its conversion into the saved setup.

use std::collections::BTreeMap;

use serde::Deserialize;

use kiki_console as console;
use kiki_toolchain as toolchain;
use kiki_tools as tools;

use crate::atlassian::AtlassianBody;
use crate::http::null_default;

#[derive(Deserialize, Default)]
#[serde(default)]
struct RepoForm {
    kind: String,
    folder: String,
    #[serde(deserialize_with = "null_default")]
    services: Vec<String>,
    divided: bool,
}

/// The body the web UI sends when Settings is saved.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct SetupForm {
    #[serde(rename = "workspaceId")]
    workspace_id: String,
    orchestrator: String,
    #[serde(rename = "generateTool")]
    generate_tool: String,
    #[serde(rename = "generateModel")]
    generate_model: String,
    #[serde(deserialize_with = "null_default")]
    repos: Vec<RepoForm>,
    pub(super) atlassian: Option<AtlassianBody>,
    pub(super) tools: Option<tools::Settings>,
}

impl SetupForm {
    /// The setup the form describes: built-in repositories that the form still has, then any other repository id.
    /// A repository the form leaves out is removed. Indexes and sparse folders are not part of the form; see [`SetupForm::keep_saved`].
    pub(super) fn snapshot(&self, root: &str) -> console::Snapshot {
        let by_kind: BTreeMap<&str, console::Repo> = self
            .repos
            .iter()
            .map(|repo| {
                let services = if repo.divided {
                    repo.services
                        .iter()
                        .map(|name| name.trim())
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                        .collect()
                } else {
                    Vec::new()
                };
                let row = console::Repo {
                    kind: repo.kind.clone(),
                    folder: repo.folder.trim().to_string(),
                    services,
                    divided: repo.divided,
                    ..Default::default()
                };
                (repo.kind.as_str(), row)
            })
            .collect();
        let mut snap = console::Snapshot {
            root: root.to_string(),
            workspace_id: self.workspace_id.trim().to_string(),
            orchestrator: self.orchestrator.trim().to_string(),
            generate_tool: self.generate_tool.trim().to_string(),
            generate_model: self.generate_model.trim().to_string(),
            ..Default::default()
        };
        for kind in toolchain::KINDS {
            if let Some(repo) = by_kind.get(kind) {
                snap.repos.push((*repo).clone());
            }
        }
        let extras = by_kind
            .iter()
            .filter(|(kind, _)| !toolchain::known(kind) && toolchain::repo_id(kind));
        snap.repos.extend(extras.map(|(_, repo)| repo.clone()));
        snap
    }

    /// Keeps what the form cannot change: each repository's name, index and sparse folders, and the saved
    /// orchestrator when the form names none.
    pub(super) fn keep_saved(snap: &mut console::Snapshot, current: &console::Snapshot) {
        for repo in snap.repos.iter_mut() {
            let kept = current.repo(&repo.kind);
            repo.name = kept.name;
            repo.index = kept.index;
            repo.sparse = kept.sparse;
        }
        if snap.orchestrator.is_empty() {
            snap.orchestrator = current.orchestrator.clone();
        }
        snap.orchestrator = console::normalize_orchestrator(&snap.orchestrator);
    }
}
