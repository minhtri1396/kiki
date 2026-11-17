//! The git branch name of a task: its Jira ticket and an English slug made from the task name.

use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Result;
use kiki_aitool::AiTool;
use kiki_console as console;
use kiki_orca as orca;
use kiki_store as store;
use kiki_util::Ctx;
use regex::Regex;

use super::error::{task_error, user_error};
use crate::api_error::{ApiError, ApiResult};
use crate::Server;

/// How long the AI tool may take to write the English name.
const NAMING_TIMEOUT: Duration = Duration::from_secs(2 * 60);
/// A branch slug is cut to this many bytes.
const MAX_SLUG: usize = 80;

/// What the branch call answers: the task, the slug to branch from and the ticket found in the task.
pub(crate) struct TaskBranch {
    pub task_id: String,
    pub slug: String,
    pub ticket: String,
}

fn jira_ticket_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    // `\b` stays ASCII only so that a ticket next to a Vietnamese word still counts.
    PATTERN.get_or_init(|| Regex::new(r"(?i)(?-u:\b)[A-Z][A-Z0-9]{1,9}-[0-9]+(?-u:\b)").expect("jira ticket pattern"))
}

/// The first Jira ticket key in `text`, upper case, or empty.
pub(crate) fn jira_ticket(text: &str) -> String {
    jira_ticket_pattern()
        .find(text)
        .map(|m| m.as_str())
        .unwrap_or("")
        .to_uppercase()
}

/// The Jira ticket of a task: its name first, then its description.
pub(crate) fn task_ticket(task: &store::Task) -> String {
    let ticket = jira_ticket(&task.name);
    if ticket.is_empty() {
        jira_ticket(&task.description)
    } else {
        ticket
    }
}

/// Lower-case ASCII words joined by `_`, taken from the first line of `value`.
pub(crate) fn branch_slug(value: &str) -> String {
    let mut value = value.trim();
    if let Some(cut) = value.find(['\r', '\n']) {
        value = &value[..cut];
    }
    let value = value.trim_matches(|c| matches!(c, ' ' | '\t' | '`' | '"' | '\''));
    let mut slug = String::new();
    let mut underscore = false;
    for ch in value.to_lowercase().chars() {
        let ch = if ch == 'đ' { 'd' } else { ch };
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            slug.push(ch);
            underscore = false;
        } else if !slug.is_empty() && !underscore {
            slug.push('_');
            underscore = true;
        }
    }
    let mut slug = slug.trim_matches('_').to_string();
    if slug.len() > MAX_SLUG {
        slug = slug[..MAX_SLUG].trim_end_matches('_').to_string();
    }
    slug
}

impl Server {
    /// Gives the task a branch slug (asking the AI tool for an English name when the stored one is missing,
    /// stale or invalid) and reports it with the task's ticket.
    pub(crate) fn task_branch(&self, ctx: &Ctx, id: &str) -> ApiResult<TaskBranch> {
        let mut task = self.store.task(id).map_err(|err| task_error(&err))?;
        let ticket = task_ticket(&task);
        let mut slug = task.branch_slug.clone();
        if slug.is_empty() || task.branch_for != task.name || !orca::valid_branch_slug(&slug) {
            slug = self
                .english_branch_slug(ctx, &task.name)
                .map_err(|err| naming_error(&err))?;
            if !orca::valid_branch_slug(&slug) {
                return Err(user_error("The branch name could not be made from this task name."));
            }
            task = self
                .store
                .set_task_branch(id, &slug, &task.name)
                .map_err(|err| task_error(&err))?;
            slug = task.branch_slug.clone();
        }
        Ok(TaskBranch {
            task_id: task.id,
            slug,
            ticket,
        })
    }

    pub(crate) fn english_branch_slug(&self, ctx: &Ctx, name: &str) -> Result<String> {
        let english = match &self.branch_namer {
            // An injected namer ignores the Generate text setup, so it needs no real tool.
            Some(namer) => namer(&ctx.with_timeout(NAMING_TIMEOUT), AiTool::Cursor, "", name)?,
            None => {
                let (tool, model) = self.generate_choice()?;
                let request = console::GenerateRequest {
                    tool,
                    model: &model,
                    kind: "branch",
                    subject: "",
                    text: name,
                };
                console::generate_text(&ctx.with_timeout(NAMING_TIMEOUT), &request)?
            }
        };
        Ok(branch_slug(&english))
    }
}

/// A Generate text setup the user must fix is 400, any other failure of the AI tool 500.
fn naming_error(err: &anyhow::Error) -> ApiError {
    if err
        .chain()
        .any(|cause| cause.downcast_ref::<console::InputError>().is_some())
    {
        return ApiError::from_anyhow(400, err);
    }
    ApiError::from_anyhow(500, err)
}
