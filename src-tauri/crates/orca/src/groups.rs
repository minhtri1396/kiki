//! Orca sidebar folders ("project groups"): putting a project in a folder, and deleting a folder.

use crate::backend::Backend;
use crate::projects::Projects;
use crate::wire::{nd, parse};
use anyhow::{anyhow, Result};
use kiki_util::Ctx;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct Group {
    #[serde(deserialize_with = "nd")]
    pub(crate) id: String,
    #[serde(deserialize_with = "nd")]
    pub(crate) name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Listed {
    #[serde(deserialize_with = "nd")]
    groups: Vec<Group>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CreatedGroup {
    #[serde(deserialize_with = "nd")]
    id: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Created {
    #[serde(deserialize_with = "nd")]
    id: String,
    #[serde(deserialize_with = "nd")]
    group: CreatedGroup,
}

pub(crate) struct Groups {
    backend: Backend,
}

impl Groups {
    pub(crate) fn new(backend: Backend) -> Groups {
        Groups { backend }
    }

    /// Puts the project into the folder `name`. With a CLI binary the CLI does it; otherwise the runtime does.
    pub(crate) fn ensure(&self, ctx: &Ctx, name: &str, project: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("feature folder name is required"));
        }
        if self.backend.cli.has_binary() {
            self.backend.cli.run(
                ctx,
                &["project", "group", "--name", name, "--project", project, "--json"],
            )?;
            return Ok(());
        }
        self.group_project(ctx, name, project)
    }

    /// Puts the named project in one Orca sidebar folder. The folder is created when missing.
    /// projectGroup.moveProject addresses the registered repo. A durable project id
    /// (github:owner/repo or repo:<id>) is not a repo selector and comes back as repo_not_found.
    fn group_project(&self, ctx: &Ctx, name: &str, display_name: &str) -> Result<()> {
        let repo_id = self.repo_selector(ctx, display_name)?;
        let group_id = match self.find_group(ctx, name)? {
            Some(id) => id,
            None => self.create_group(ctx, name)?,
        };
        let params = json!({
            "repo": repo_id,
            "groupId": group_id,
        });
        self.backend
            .runtime(ctx, "projectGroup.moveProject", Some(&params))
            .map_err(|e| anyhow!("group Orca projects in {name}: {e:#}"))?;
        Ok(())
    }

    /// The selector Orca accepts for the project: its first source repo id, or its display name.
    fn repo_selector(&self, ctx: &Ctx, display_name: &str) -> Result<String> {
        let projects = Projects::new(self.backend.clone()).list(ctx)?;
        let project = projects
            .iter()
            .find(|p| p.display_name == display_name)
            .ok_or_else(|| anyhow!("Orca project {display_name} was not found"))?;
        Ok(project
            .source_repo_ids
            .first()
            .cloned()
            .unwrap_or_else(|| display_name.to_string()))
    }

    fn find_group(&self, ctx: &Ctx, name: &str) -> Result<Option<String>> {
        let raw = self
            .backend
            .runtime(ctx, "projectGroup.list", None)
            .map_err(|e| anyhow!("group Orca projects in {name}: {e:#}"))?;
        let listed: Listed = parse(raw.to_string().as_bytes())
            .map_err(|_| anyhow!("group Orca projects in {name}: Orca returned an unreadable response"))?;
        Ok(listed
            .groups
            .into_iter()
            .find(|g| g.name == name)
            .map(|g| g.id)
            .filter(|id| !id.is_empty()))
    }

    fn create_group(&self, ctx: &Ctx, name: &str) -> Result<String> {
        let params = json!({
            "name": name,
            "createdFrom": "manual",
        });
        let raw = self
            .backend
            .runtime(ctx, "projectGroup.create", Some(&params))
            .map_err(|e| anyhow!("group Orca projects in {name}: {e:#}"))?;
        let created: Created = parse(raw.to_string().as_bytes())
            .map_err(|_| anyhow!("group Orca projects in {name}: Orca returned an unreadable response"))?;
        let id = if created.group.id.is_empty() {
            created.id
        } else {
            created.group.id
        };
        if id.is_empty() {
            return Err(anyhow!("group Orca projects in {name}: Orca did not return a folder"));
        }
        Ok(id)
    }

    /// Deletes every folder named `name`; a folder that is already gone is fine.
    pub(crate) fn delete(&self, ctx: &Ctx, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        let raw = self.backend.runtime(ctx, "projectGroup.list", None)?;
        let listed: Listed = parse(raw.to_string().as_bytes())
            .map_err(|_| anyhow!("read Orca project groups: Orca returned an unreadable response"))?;
        for group in listed
            .groups
            .iter()
            .filter(|g| g.name == name && !g.id.trim().is_empty())
        {
            let params = json!({ "groupId": group.id });
            if let Err(err) = self.backend.runtime(ctx, "projectGroup.delete", Some(&params)) {
                if crate::wire::orca_gone_text(&err.to_string()) {
                    continue;
                }
                return Err(anyhow!("remove Orca folder {name}: {err:#}"));
            }
        }
        Ok(())
    }
}
