//! Orca projects: finding, adding and renaming the project that holds one repository folder.

use crate::backend::Backend;
use crate::paths::same_path;
use crate::wire::{nd, parse_result};
use anyhow::{anyhow, Result};
use kiki_util::Ctx;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub(crate) struct OrcaProject {
    #[serde(deserialize_with = "nd")]
    pub(crate) id: String,
    #[serde(rename = "displayName", deserialize_with = "nd")]
    pub(crate) display_name: String,
    #[serde(rename = "sourceRepoIds", deserialize_with = "nd")]
    pub(crate) source_repo_ids: Vec<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub(crate) struct HostSetup {
    #[serde(deserialize_with = "nd")]
    pub(crate) id: String,
    #[serde(rename = "projectId", deserialize_with = "nd")]
    pub(crate) project_id: String,
    #[serde(rename = "displayName", deserialize_with = "nd")]
    pub(crate) display_name: String,
    #[serde(deserialize_with = "nd")]
    pub(crate) path: String,
}

pub(crate) struct Projects {
    backend: Backend,
}

impl Projects {
    pub(crate) fn new(backend: Backend) -> Projects {
        Projects { backend }
    }

    pub(crate) fn list(&self, ctx: &Ctx) -> Result<Vec<OrcaProject>> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Listed {
            #[serde(deserialize_with = "nd")]
            projects: Vec<OrcaProject>,
        }
        let out = self.backend.cli.run(ctx, &["project", "list", "--json"])?;
        Ok(parse_result::<Listed>(&out, "read Orca projects")?.projects)
    }

    pub(crate) fn setups(&self, ctx: &Ctx) -> Result<Vec<HostSetup>> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Listed {
            #[serde(deserialize_with = "nd")]
            setups: Vec<HostSetup>,
        }
        let out = self.backend.cli.run(ctx, &["project", "setups", "--json"])?;
        Ok(parse_result::<Listed>(&out, "read Orca project setups")?.setups)
    }

    /// Makes sure a project named `name` holds the folder `root`: reuses it, renames the project that already
    /// holds the folder, or adds the folder.
    pub(crate) fn ensure(&self, ctx: &Ctx, root: &str, name: &str) -> Result<()> {
        let projects = self.list(ctx)?;
        let setups = self.setups(ctx)?;
        let named_id = projects
            .iter()
            .find(|p| p.display_name == name)
            .map(|p| p.id.as_str())
            .filter(|id| !id.is_empty());
        let owned = setups.iter().find(|setup| same_path(&setup.path, root));
        if let Some(named_id) = named_id {
            return match owned {
                Some(owned) if owned.project_id == named_id => Ok(()),
                _ => Err(anyhow!("Orca project {name} already exists for a different folder")),
            };
        }
        if let Some(owned) = owned {
            if owned.display_name == name {
                return Ok(());
            }
            return self.rename_setup(ctx, &owned.id, name);
        }
        if self.backend.cli.has_binary() {
            return self.add_via_cli(ctx, root, name);
        }
        self.add_folder_project(ctx, root, name)
    }

    fn add_via_cli(&self, ctx: &Ctx, root: &str, name: &str) -> Result<()> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Repo {
            #[serde(deserialize_with = "nd")]
            id: String,
            #[serde(rename = "displayName", deserialize_with = "nd")]
            display_name: String,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Added {
            #[serde(deserialize_with = "nd")]
            repo: Repo,
        }
        let out = self.backend.cli.run(ctx, &["repo", "add", "--path", root, "--json"])?;
        let repo = parse_result::<Added>(&out, "")
            .map(|added| added.repo)
            .unwrap_or_default();
        if repo.id.is_empty() {
            return Err(anyhow!("Orca registered {root} but did not return a project id"));
        }
        if repo.display_name == name {
            return Ok(());
        }
        self.rename_setup(ctx, &repo.id, name)
    }

    fn rename_setup(&self, ctx: &Ctx, id: &str, name: &str) -> Result<()> {
        self.backend.cli.run(
            ctx,
            &[
                "project",
                "setup-update",
                "--setup",
                id,
                "--display-name",
                name,
                "--json",
            ],
        )?;
        Ok(())
    }

    /// Registers a plain folder as an Orca project with the requested name.
    /// The CLI repo command only imports Git repositories and names them from the Git remote,
    /// so a folder project is created through the local Orca runtime.
    fn add_folder_project(&self, ctx: &Ctx, root: &str, name: &str) -> Result<()> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Repo {
            #[serde(rename = "displayName", deserialize_with = "nd")]
            display_name: String,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Added {
            #[serde(deserialize_with = "nd")]
            repo: Repo,
        }
        let params = json!({
            "path": root,
            "kind": "folder",
            "displayName": name,
        });
        let raw = self
            .backend
            .runtime(ctx, "repo.add", Some(&params))
            .map_err(|e| anyhow!("create Orca project {name}: {e:#}"))?;
        let added: Added = crate::wire::parse(raw.to_string().as_bytes())
            .map_err(|_| anyhow!("create Orca project {name}: Orca returned an unreadable response"))?;
        if added.repo.display_name != name {
            return Err(anyhow!(
                "create Orca project {name}: Orca named it {}",
                added.repo.display_name
            ));
        }
        Ok(())
    }
}
