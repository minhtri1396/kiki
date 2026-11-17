//! Names and validation for Orca projects, groups, worktrees and feature branches.

use anyhow::{anyhow, Result};
use regex::Regex;
use std::sync::OnceLock;

fn feature_task_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:[1-9][0-9]{0,17}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|[0-9A-HJKMNP-TV-Z]{26})$").unwrap()
    })
}

fn feature_slug_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9_]{0,79}$").unwrap())
}

fn feature_ticket_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Z][A-Z0-9]{1,9}-[0-9]+$").unwrap())
}

/// ProjectName is the Orca project that holds one repo for a workspace.
pub fn project_name(workspace: &str, repo: &str) -> Result<String> {
    let workspace = workspace.trim();
    let repo = repo.trim();
    if workspace.is_empty() || repo.is_empty() {
        return Err(anyhow!("workspace and repo are required to name the Orca project"));
    }
    Ok(format!("{workspace}-{repo}"))
}

/// ValidTaskID reports whether id can name an Orca folder, project, or worktree.
/// Task ids and group ids share this shape.
pub fn valid_task_id(id: &str) -> bool {
    feature_task_pattern().is_match(id.trim())
}

/// ValidBranchSlug reports whether slug can sit in a feature branch name.
pub fn valid_branch_slug(slug: &str) -> bool {
    feature_slug_pattern().is_match(slug.trim())
}

/// ValidTicket reports whether ticket is empty or a Jira issue key.
pub fn valid_ticket(ticket: &str) -> bool {
    let ticket = ticket.trim().to_uppercase();
    ticket.is_empty() || feature_ticket_pattern().is_match(&ticket)
}

/// FeatureProjectName is the Orca project for one repo of one group.
/// The parts are the workspace, the group id, and the repo.
pub fn feature_project_name(workspace: &str, group_id: &str, repo: &str) -> Result<String> {
    let workspace = workspace.trim();
    let group_id = group_id.trim();
    let repo = repo.trim();
    if workspace.is_empty() || repo.is_empty() {
        return Err(anyhow!("workspace and repo are required to name the Orca project"));
    }
    if !feature_task_pattern().is_match(group_id) {
        return Err(anyhow!("group id is not valid"));
    }
    Ok(format!("{workspace}-{group_id}-{repo}"))
}

/// FeatureBranch is the git branch for the worktree created for one repo of a task.
/// A Jira key is included when the task has one. The document repo is spelled docs.
pub fn feature_branch(ticket: &str, slug: &str, kind: &str) -> Result<String> {
    let ticket = ticket.trim().to_uppercase();
    let slug = slug.trim();
    if !valid_ticket(&ticket) || !feature_slug_pattern().is_match(slug) {
        return Err(anyhow!("feature name is not valid"));
    }
    let repo = branch_repo(kind);
    if repo.is_empty() || repo.contains([' ', '/']) {
        return Err(anyhow!("feature repo is not valid"));
    }
    let mut name = format!("{slug}-{repo}");
    if !ticket.is_empty() {
        name = format!("{ticket}-{name}");
    }
    Ok(format!("feat/{name}"))
}

fn branch_repo(kind: &str) -> String {
    let kind = kind.trim();
    if kind == "document" {
        return "docs".to_string();
    }
    kind.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_names() {
        let project = feature_project_name("studio", "12", "backend").unwrap();
        assert_eq!(project, "studio-12-backend");
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let named = feature_project_name("studio", id, "backend").unwrap();
        assert_eq!(named, format!("studio-{id}-backend"));
        let ulid = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let named = feature_project_name("studio", ulid, "backend").unwrap();
        assert_eq!(named, format!("studio-{ulid}-backend"));
        assert!(
            feature_project_name("studio", "0", "backend").is_err(),
            "group id 0 was accepted"
        );
        assert!(
            valid_task_id(id) && !valid_task_id("550E8400-E29B-41D4-A716-446655440000"),
            "uuid task id check failed"
        );
        assert!(
            valid_task_id(ulid) && !valid_task_id("01ARZ3NDEKTSV4RRFFQ69G5FAI") && !valid_task_id(&ulid.to_lowercase()),
            "ulid task id check failed"
        );
        assert!(
            feature_project_name("", "12", "backend").is_err(),
            "empty workspace was accepted"
        );

        let branch = feature_branch("ge-12", "add_shopping_cart", "document").unwrap();
        assert_eq!(branch, "feat/GE-12-add_shopping_cart-docs");
        let plain = feature_branch("", "add_cart", "backend").unwrap();
        assert_eq!(plain, "feat/add_cart-backend");
        assert!(
            feature_branch("not-a-key", "add_cart", "backend").is_err(),
            "ticket was accepted"
        );
        assert!(feature_branch("", "Add Cart", "backend").is_err(), "slug was accepted");
    }

    #[test]
    fn project_name_needs_both_parts() {
        assert_eq!(project_name(" ws ", " repo ").unwrap(), "ws-repo");
        assert_eq!(
            project_name("", "repo").unwrap_err().to_string(),
            "workspace and repo are required to name the Orca project"
        );
        assert!(valid_ticket("") && valid_ticket("ge-12") && !valid_ticket("nope"));
        assert!(valid_branch_slug("add_cart") && !valid_branch_slug("Add"));
    }
}
