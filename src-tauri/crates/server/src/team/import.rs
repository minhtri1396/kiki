//! Pulling from a team folder: importing a member's agents (resolving the repositories this app lacks) and
//! adopting shared repository files.

use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Result};
use kiki_agentfile as agentfile;
use kiki_console as console;
use kiki_gitignore as gitignore;
use kiki_teamfolder as teamfolder;
use kiki_toolchain as toolchain;
use kiki_util::path;
use serde::Deserialize;
use serde_json::json;

use super::shared_repo::{list_shared_repos, merge_shared, remove_extra_repo_files};
use super::Outcome;
use crate::Server;

/// What the request chose for one repository id that the pulled agents use but this app does not have.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub(crate) struct RepoDecision {
    pub action: String,
    pub id: String,
}

/// The decisions of a request, by repository id.
pub(crate) type RepoDecisions = BTreeMap<String, RepoDecision>;

/// How the unknown repository ids are settled.
#[derive(Debug, PartialEq, Eq)]
enum RepoPlan {
    /// Every id has a decision: rewrite the `replacements` in the agent files and create the `create` ids.
    Ready {
        replacements: BTreeMap<String, String>,
        create: Vec<String>,
    },
    /// These ids still need a decision from the user.
    Missing(Vec<String>),
}

/// The sorted repository kinds the agent files use that are not in `known`.
fn unknown_repo_kinds(files: &[agentfile::File], known: &HashSet<&str>) -> Vec<String> {
    let mut unknown: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for repo in files.iter().flat_map(|file| &file.repos) {
        let kind = repo.kind.trim();
        if kind.is_empty() || known.contains(kind) || !seen.insert(kind) {
            continue;
        }
        unknown.push(kind.to_string());
    }
    unknown.sort();
    unknown
}

/// Turns the request's decisions into a plan. An invalid decision is an error; an id without one is `Missing`.
fn plan_repo_choices(unknown: &[String], decisions: &RepoDecisions, known: &HashSet<&str>) -> Result<RepoPlan> {
    let mut replacements: BTreeMap<String, String> = BTreeMap::new();
    let mut create: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for id in unknown {
        let Some(choice) = decisions.get(id) else {
            missing.push(id.clone());
            continue;
        };
        match choice.action.trim() {
            "create" => {
                if !toolchain::repo_id(id) {
                    return Err(anyhow!("repository {id} cannot be created"));
                }
                create.push(id.clone());
            }
            "use" => {
                let target = choice.id.trim();
                if !known.contains(target) {
                    return Err(anyhow!("choose an existing repository for {id}"));
                }
                replacements.insert(id.clone(), target.to_string());
            }
            _ => missing.push(id.clone()),
        }
    }
    if missing.is_empty() {
        Ok(RepoPlan::Ready { replacements, create })
    } else {
        Ok(RepoPlan::Missing(missing))
    }
}

impl Server {
    /// Replaces `dest` with the agent files in `source`. A repository id this app does not have is returned as a
    /// conflict until the request chooses to create it or to use an existing repository.
    pub(super) fn import_agents(&self, source: &str, dest: &str, decisions: &RepoDecisions) -> Result<Outcome> {
        let snap = self.load()?;
        let known: HashSet<&str> = snap.repos.iter().map(|repo| repo.kind.as_str()).collect();
        let files = agentfile::list(source)?;
        let unknown = unknown_repo_kinds(&files, &known);
        match plan_repo_choices(&unknown, decisions, &known)? {
            RepoPlan::Missing(missing) => {
                let repo_ids: Vec<&str> = snap.repos.iter().map(|repo| repo.kind.as_str()).collect();
                Ok(Outcome::Conflict(json!({
                    "error": "Choose a repository for each one this app does not have.",
                    "unknown": missing,
                    "repos": repo_ids,
                })))
            }
            RepoPlan::Ready { replacements, create } => {
                teamfolder::copy(source, dest)?;
                agentfile::replace_repos(dest, &replacements)?;
                self.ensure_repos(&create)?;
                Ok(Outcome::Done)
            }
        }
    }

    /// Replaces this app's repository settings with the shared files in `src` (a member's `repos` folder).
    pub(super) fn adopt_repos(&self, src: &str) -> Result<()> {
        let shared = list_shared_repos(src)?;
        if shared.is_empty() {
            return Err(anyhow!("That folder has no repositories to copy."));
        }
        let mut snap = self.load()?;
        let (next, added) = merge_shared(&snap.repos, shared);
        snap.repos = next;
        let snap = self.persist_repos(snap)?;
        let keep: HashSet<&str> = snap.repos.iter().map(|repo| repo.kind.as_str()).collect();
        remove_extra_repo_files(&path::join(&[&path::dir(self.store.file()), "repos"]), &keep)?;
        self.ignore_repo_folders(&added)
    }

    /// Adds a repository for each id that this app does not have yet.
    fn ensure_repos(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut snap = self.load()?;
        let mut existing: HashSet<String> = snap.repos.iter().map(|repo| repo.kind.clone()).collect();
        let mut added: Vec<String> = Vec::new();
        for id in ids {
            if existing.contains(id) {
                continue;
            }
            if !toolchain::repo_id(id) {
                return Err(anyhow!("repository {id} cannot be created"));
            }
            snap.repos.push(console::Repo {
                kind: id.clone(),
                ..Default::default()
            });
            existing.insert(id.clone());
            added.push(id.clone());
        }
        if added.is_empty() {
            return Ok(());
        }
        self.persist_repos(snap)?;
        self.ignore_repo_folders(&added)
    }

    /// Gives every repository a folder, validates the setup and saves it.
    fn persist_repos(&self, snap: console::Snapshot) -> Result<console::Snapshot> {
        let snap = console::apply_root(&self.root, snap)?;
        console::validate(&snap)?;
        self.store.save(&snap)?;
        Ok(snap)
    }

    /// Keeps the new repositories' folders under the app root out of git.
    fn ignore_repo_folders(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let entries: Vec<gitignore::Entry> = ids
            .iter()
            .map(|id| gitignore::Entry {
                path: path::join(&[&self.root, id]),
                dir: true,
            })
            .collect();
        gitignore::ensure(&self.root, &entries)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(action: &str, id: &str) -> RepoDecision {
        RepoDecision {
            action: action.into(),
            id: id.into(),
        }
    }

    fn known<'a>(ids: &[&'a str]) -> HashSet<&'a str> {
        ids.iter().copied().collect()
    }

    #[test]
    fn undecided_ids_are_reported_missing() {
        let unknown = vec!["payments".to_string(), "search".to_string()];
        let decisions = RepoDecisions::from([("payments".to_string(), decision("create", ""))]);
        let plan = plan_repo_choices(&unknown, &decisions, &known(&["backend"])).unwrap();
        assert_eq!(plan, RepoPlan::Missing(vec!["search".to_string()]));
    }

    #[test]
    fn an_unknown_action_counts_as_no_decision() {
        let unknown = vec!["payments".to_string()];
        let decisions = RepoDecisions::from([("payments".to_string(), decision("skip", ""))]);
        let plan = plan_repo_choices(&unknown, &decisions, &known(&[])).unwrap();
        assert_eq!(plan, RepoPlan::Missing(unknown));
    }

    #[test]
    fn decisions_become_replacements_and_new_repositories() {
        let unknown = vec!["payments".to_string(), "search".to_string()];
        let decisions = RepoDecisions::from([
            ("payments".to_string(), decision("use", " backend ")),
            ("search".to_string(), decision("create", "")),
        ]);
        let plan = plan_repo_choices(&unknown, &decisions, &known(&["backend"])).unwrap();
        let replacements = BTreeMap::from([("payments".to_string(), "backend".to_string())]);
        assert_eq!(
            plan,
            RepoPlan::Ready {
                replacements,
                create: vec!["search".to_string()]
            }
        );
    }

    #[test]
    fn invalid_decisions_keep_their_error_text() {
        let unknown = vec!["Bad-Id".to_string()];
        let create = RepoDecisions::from([("Bad-Id".to_string(), decision("create", ""))]);
        let err = plan_repo_choices(&unknown, &create, &known(&[])).unwrap_err();
        assert_eq!(err.to_string(), "repository Bad-Id cannot be created");
        let unknown = vec!["payments".to_string()];
        let reuse = RepoDecisions::from([("payments".to_string(), decision("use", "nope"))]);
        let err = plan_repo_choices(&unknown, &reuse, &known(&["backend"])).unwrap_err();
        assert_eq!(err.to_string(), "choose an existing repository for payments");
    }

    #[test]
    fn unknown_kinds_are_trimmed_unique_and_sorted() {
        let file = |kinds: &[&str]| agentfile::File {
            repos: kinds
                .iter()
                .map(|k| agentfile::Repository {
                    kind: k.to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let files = vec![file(&["zeta", " alpha ", "backend", ""]), file(&["alpha", "zeta"])];
        assert_eq!(unknown_repo_kinds(&files, &known(&["backend"])), vec!["alpha", "zeta"]);
    }
}
