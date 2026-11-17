//! Creating and deleting a group together with its Jira epic.

use std::time::Duration;

use anyhow::Result;
use kiki_store as store;
use kiki_util::Ctx;

use crate::api_error::{ApiError, ApiResult};
use crate::tasks::{task_error, user_error};
use crate::Server;

/// How long Jira may take to create or delete the epic.
const JIRA_TIMEOUT: Duration = Duration::from_secs(45);

/// Jira's refusal is shown to the user as it is, like any other mistake they can fix.
fn jira_refused(err: &anyhow::Error) -> anyhow::Error {
    store::TaskError::new(format!("{err:#}")).into()
}

impl Server {
    /// Creates a group. With Jira on, the group gets an epic named like it.
    pub(crate) fn create_group(&self, ctx: &Ctx, name: &str, description: &str) -> ApiResult<store::Group> {
        let (name, description) = (name.trim(), description.trim());
        if name.is_empty() {
            return Err(user_error("Enter a group name."));
        }
        let cfg = self.store.atlassian().map_err(|err| task_error(&err))?;
        let mut input = store::GroupInput {
            name: name.to_string(),
            description: description.to_string(),
            ..Default::default()
        };
        if cfg.enabled && cfg.jira.enabled {
            let ctx = ctx.with_timeout(JIRA_TIMEOUT);
            let epic = self
                .atlassian
                .create_epic(&ctx, &cfg, name, description)
                .map_err(|err| task_error(&jira_refused(&err)))?;
            input.epic_key = epic.key;
            input.epic_board_id = epic.board_id;
            input.epic_board_name = epic.board_name;
        }
        self.store.create_group(input).map_err(|err| task_error(&err))
    }

    /// Deletes a group. With `jira` set the group's epic is deleted first, and the group stays when Jira refuses.
    pub(crate) fn delete_group(&self, ctx: &Ctx, id: &str, jira: bool) -> ApiResult<()> {
        if jira {
            self.delete_group_epic(ctx, id).map_err(|err| {
                if store::is_group_not_found(&err) {
                    ApiError::from_anyhow(404, &err)
                } else {
                    task_error(&err)
                }
            })?;
        }
        self.store.delete_group(id).map_err(|err| {
            let status = if store::is_group_not_found(&err) { 404 } else { 500 };
            ApiError::from_anyhow(status, &err)
        })
    }

    /// Removes the Jira epic stored on the group. A group with no epic has nothing to remove.
    pub(crate) fn delete_group_epic(&self, ctx: &Ctx, id: &str) -> Result<()> {
        let group = self.store.group(id)?;
        let key = group.epic_key.trim();
        if key.is_empty() {
            return Ok(());
        }
        let cfg = self.store.atlassian()?;
        self.atlassian
            .delete_issue(&ctx.with_timeout(JIRA_TIMEOUT), &cfg, key)
            .map_err(|err| jira_refused(&err))
    }
}
