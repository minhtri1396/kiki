//! Checks a snapshot before it is stored.

use std::collections::BTreeSet;
use std::fs;
use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use regex::Regex;

use kiki_aitool::AiTool;
use kiki_toolchain as toolchain;
use kiki_util::path;

use crate::orchestrator::known_orchestrator;
use crate::services::SERVICE_NAME;
use crate::snapshot::{Repo, Snapshot};
use crate::tool::MODEL_ID;

static WORKSPACE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{0,31}$").unwrap());

/// Checks a snapshot before it is stored.
pub fn validate(snap: &Snapshot) -> Result<()> {
    validate_options(snap)?;
    validate_repos(&snap.repos)
}

/// The workspace id, the orchestrator, and the Generate tool and model.
fn validate_options(snap: &Snapshot) -> Result<()> {
    if !WORKSPACE_ID.is_match(&snap.workspace_id) {
        return Err(anyhow!(
            "workspace must be lowercase letters, digits, hyphens, or underscores"
        ));
    }
    if !known_orchestrator(&snap.orchestrator) {
        return Err(anyhow!("Agent Orchestration tool is not supported"));
    }
    if !snap.generate_tool.is_empty() && AiTool::parse(&snap.generate_tool).is_none() {
        return Err(anyhow!("Generate AI Tool is not supported"));
    }
    if !snap.generate_model.is_empty() && !MODEL_ID.is_match(&snap.generate_model) {
        return Err(anyhow!("Generate model is not supported"));
    }
    Ok(())
}

/// Distinct repository ids. A document repository carries service names only when it is divided into services.
fn validate_repos(repos: &[Repo]) -> Result<()> {
    let mut seen_kind: BTreeSet<String> = BTreeSet::new();
    for repo in repos {
        if !toolchain::repo_id(&repo.kind) || !seen_kind.insert(repo.kind.clone()) {
            return Err(anyhow!("repository {} is not valid", repo.kind));
        }
        validate_repo(repo, repo.kind == "document")?;
    }
    Ok(())
}

fn validate_repo(repo: &Repo, document: bool) -> Result<()> {
    if let Err(err) = validate_folder(&repo.folder) {
        return Err(anyhow!("{}: {:#}", repo.kind, err));
    }
    if document && !repo.divided && !repo.services.is_empty() {
        return Err(anyhow!("document does not use service names"));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for name in &repo.services {
        if !SERVICE_NAME.is_match(name) {
            return Err(anyhow!(
                "service name {:?} for {} may contain only letters, digits, dots, hyphens, or underscores",
                name,
                repo.kind
            ));
        }
        if !seen.insert(name) {
            return Err(anyhow!("service {:?} for {} is duplicated", name, repo.kind));
        }
    }
    Ok(())
}

fn validate_folder(raw: &str) -> Result<()> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(());
    }
    if raw.contains(['\r', '\n']) {
        return Err(anyhow!("folder path is invalid"));
    }
    if !path::is_abs(raw) {
        return Err(anyhow!("folder must be an absolute path"));
    }
    match fs::metadata(raw) {
        Ok(info) if info.is_dir() => Ok(()),
        _ => Err(anyhow!("folder {} does not exist", raw)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(kind: &str) -> Repo {
        Repo {
            kind: kind.into(),
            ..Default::default()
        }
    }

    fn valid() -> Snapshot {
        Snapshot {
            workspace_id: "kiki".into(),
            repos: ["backend", "website", "mobile", "document"]
                .into_iter()
                .map(repo)
                .collect(),
            ..Default::default()
        }
    }

    fn message(snap: &Snapshot) -> String {
        validate(snap).unwrap_err().to_string()
    }

    #[test]
    fn a_complete_snapshot_is_valid() {
        validate(&valid()).unwrap();
    }

    #[test]
    fn options_are_checked_in_order() {
        let mut snap = valid();
        snap.workspace_id = "Bad Id".into();
        assert_eq!(
            message(&snap),
            "workspace must be lowercase letters, digits, hyphens, or underscores"
        );
        let mut snap = valid();
        snap.orchestrator = "other".into();
        assert_eq!(message(&snap), "Agent Orchestration tool is not supported");
        let mut snap = valid();
        snap.generate_tool = "vim".into();
        assert_eq!(message(&snap), "Generate AI Tool is not supported");
        snap.generate_tool = "claude".into();
        validate(&snap).unwrap();
        snap.generate_model = "bad model".into();
        assert_eq!(message(&snap), "Generate model is not supported");
    }

    #[test]
    fn repositories_keep_distinct_ids() {
        let mut snap = valid();
        snap.repos.truncate(3);
        validate(&snap).unwrap();
        let mut snap = valid();
        snap.repos.swap(0, 1);
        validate(&snap).unwrap();
        let mut snap = valid();
        snap.repos.clear();
        validate(&snap).unwrap();
        let mut snap = valid();
        snap.repos.push(repo("backend"));
        assert_eq!(message(&snap), "repository backend is not valid");
        let mut snap = valid();
        snap.repos.push(repo("extra"));
        validate(&snap).unwrap();
        snap.repos.push(repo("extra"));
        assert_eq!(message(&snap), "repository extra is not valid");
        let mut snap = valid();
        snap.repos.push(repo("Bad Id"));
        assert_eq!(message(&snap), "repository Bad Id is not valid");
    }

    #[test]
    fn folders_and_services_are_checked() {
        let mut snap = valid();
        snap.repos[0].folder = "relative".into();
        assert_eq!(message(&snap), "backend: folder must be an absolute path");
        snap.repos[0].folder = "/kiki-no-such-folder".into();
        assert_eq!(message(&snap), "backend: folder /kiki-no-such-folder does not exist");
        let mut snap = valid();
        snap.repos[3].services = vec!["api".into()];
        assert_eq!(message(&snap), "document does not use service names");
        let mut snap = valid();
        snap.repos[0].services = vec!["api".into(), "api".into()];
        assert_eq!(message(&snap), "service \"api\" for backend is duplicated");
        snap.repos[0].services = vec!["bad name".into()];
        assert_eq!(
            message(&snap),
            "service name \"bad name\" for backend may contain only letters, digits, dots, hyphens, or underscores"
        );
    }
}
