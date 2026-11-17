//! The files KIKI keeps in the .kiki folder: `Store` methods (facade) for the setup file; the per-repository
//! files, tool settings and MCP credentials live in their own modules.
//!
//! * `doc`: kiki.yaml. * `lenient`: yaml.v3-style readers. * `repos`: `repos/<id>.yaml`.
//! * `tools`: tools.yaml. * `atlassian`: mcp.yaml. * `legacy`: files an older KIKI wrote beside the app.

mod atlassian;
mod doc;
mod legacy;
pub(crate) mod lenient;
mod repos;
mod tools;

use anyhow::Result;
use kiki_console as console;

pub use doc::TeamFolder;
pub(crate) use doc::{default_doc, parse_doc, read_doc, write_doc, RepoDoc};
pub use legacy::migrate;
pub use repos::remove_index_folder;

use crate::store::Store;
use doc::{repos_equal, repos_from};
use repos::{merge_repo_files, repos_for_files, write_repo_files};

impl Store {
    /// EnsureLayout creates backend, website, mobile, and docs under root when they
    /// are missing, and stores those paths. Service names already in the file stay.
    pub fn ensure_layout(&self, root: &str) -> Result<()> {
        let _guard = self.lock();
        let mut doc = read_doc(&self.path)?;
        let snap = console::apply_root(root, doc.snapshot())?;
        let snap = merge_repo_files(&self.path, snap)?;
        write_repo_files(&self.path, &repos_for_files(&snap))?;
        let next = repos_from(&snap);
        if repos_equal(&doc.repos, &next) {
            return Ok(());
        }
        doc.repos = next;
        write_doc(&self.path, &doc)
    }

    /// Load reads the saved setup. Root is filled by the caller.
    pub fn load(&self) -> Result<console::Snapshot> {
        let _guard = self.lock();
        let doc = read_doc(&self.path)?;
        merge_repo_files(&self.path, doc.snapshot())
    }

    /// TeamFolder returns the saved local team folder.
    pub fn team_folder(&self) -> Result<TeamFolder> {
        let _guard = self.lock();
        Ok(read_doc(&self.path)?.team_folder)
    }

    /// SaveTeamFolder stores the team folder without changing the rest of the setup.
    pub fn save_team_folder(&self, cfg: &TeamFolder) -> Result<()> {
        let _guard = self.lock();
        let mut doc = read_doc(&self.path)?;
        doc.team_folder = cfg.trimmed();
        write_doc(&self.path, &doc)
    }

    /// Save replaces the setup. Run history stays in the SQLite file.
    pub fn save(&self, snap: &console::Snapshot) -> Result<()> {
        let _guard = self.lock();
        let mut doc = read_doc(&self.path)?;
        doc.workspace_id = snap.workspace_id.clone();
        doc.orchestrator = console::normalize_orchestrator(&snap.orchestrator);
        doc.agent_path = snap.agent_path.clone();
        doc.generate_tool = snap.generate_tool.trim().to_string();
        doc.generate_model = snap.generate_model.trim().to_string();
        doc.repos = repos_from(snap);
        write_doc(&self.path, &doc)?;
        write_repo_files(&self.path, &repos_for_files(snap))
    }
}
