//! Add, rename, and delete repository names from the Repositories tab.
//!
//! A repository has a display name and an id. Creating one builds the id from the name: lowercase,
//! with words separated by hyphens. Renaming changes the display name and leaves the id as it is.
//! The repositories the app creates at the start can be renamed and deleted.

use anyhow::{anyhow, Result};
use kiki_agentfile as agentfile;
use kiki_console as console;
use kiki_toolchain as toolchain;
use serde::Deserialize;

use super::Server;
use crate::api_error::{ApiResult, OrStatus};
use crate::http::Response;

/// The name field the Repositories tab sends.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct RepoNameBody {
    name: String,
    divided: bool,
}

impl Server {
    /// Creates a repository from the name in the body and returns the new state.
    pub(crate) fn create_repo(&self, body: RepoNameBody) -> ApiResult<Response> {
        self.add_named_repo(&body.name, body.divided).or_status(400)?;
        self.state_reply()
    }

    /// Renames the repository `kind` to the name in the body and returns the new state.
    pub(crate) fn rename_repo(&self, kind: &str, body: RepoNameBody) -> ApiResult<Response> {
        self.rename_named_repo(kind, &body.name, body.divided).or_status(400)?;
        self.state_reply()
    }

    /// Deletes the repository `kind` and returns the new state.
    /// The folder on disk stays. An agent that uses only this repository blocks the delete.
    pub(crate) fn delete_repo(&self, kind: &str) -> ApiResult<Response> {
        self.delete_named_repo(kind).or_status(400)?;
        self.state_reply()
    }
}

impl Server {
    fn add_named_repo(&self, raw: &str, divided: bool) -> Result<()> {
        let (name, id) = repo_name(raw)?;
        let snap = self.load()?;
        if let Some(title) = taken(&snap, &id, "") {
            return Err(anyhow!("This name matches \"{title}\"."));
        }
        let mut snap = snap;
        snap.repos.push(console::Repo {
            kind: id.clone(),
            name,
            divided,
            ..Default::default()
        });
        self.persist_repos(snap)?;
        self.ignore_repo_folders(&[id])
    }

    /// Changes the display name. The id assigned when the repository was created stays.
    fn rename_named_repo(&self, kind: &str, raw: &str, divided: bool) -> Result<()> {
        let kind = require_editable(kind)?;
        let (name, _) = repo_name(raw)?;
        let mut snap = self.load()?;
        if !snap.repos.iter().any(|repo| repo.kind == kind) {
            return Err(anyhow!("unknown repository"));
        }
        for repo in snap.repos.iter_mut() {
            if repo.kind == kind {
                repo.name = name.clone();
                repo.divided = divided;
                if !divided {
                    repo.services.clear();
                }
            }
        }
        self.persist_repos(snap)?;
        Ok(())
    }

    fn delete_named_repo(&self, kind: &str) -> Result<()> {
        let kind = require_editable(kind)?;
        let mut snap = self.load()?;
        if !snap.repos.iter().any(|repo| repo.kind == kind) {
            return Err(anyhow!("unknown repository"));
        }
        let dir = self.agent_dir()?;
        agentfile::forget_repos(&dir, &[kind])?;
        snap.repos.retain(|repo| repo.kind != kind);
        let snap = self.persist_repos(snap)?;
        self.prune_repo_files(&snap)
    }
}

/// The display name and the id stored for a name the user typed.
/// The name is kept as typed. The id is lowercase, and a word boundary becomes `-`.
/// `Automation Backend` and `AutomationBackend` both use the id `automation-backend`.
fn repo_name(raw: &str) -> Result<(String, String)> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(anyhow!("Enter a repository name."));
    }
    const REJECTED: &str =
        "Use a name that starts with a letter and uses only letters, digits, spaces, hyphens, or underscores.";
    let mut split = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if !(ch.is_ascii_alphanumeric() || ch == ' ' || ch == '-' || ch == '_') {
            return Err(anyhow!(REJECTED));
        }
        if i > 0 {
            let prev = chars[i - 1];
            if (prev.is_ascii_lowercase() || prev.is_ascii_digit()) && ch.is_ascii_uppercase() {
                split.push('-');
            }
        }
        split.push(ch);
    }
    let mut id = String::new();
    let mut hyphen = true;
    for ch in split.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
            hyphen = false;
        } else if !hyphen {
            id.push('-');
            hyphen = true;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    if !toolchain::repo_id(&id) {
        return Err(anyhow!(REJECTED));
    }
    Ok((name.to_string(), id))
}

/// The display name of another repository that already uses `id`. `current` is the repository being edited.
fn taken(snap: &console::Snapshot, id: &str, current: &str) -> Option<String> {
    snap.repos
        .iter()
        .find(|repo| repo.kind == id && repo.kind != current)
        .map(|repo| repo.title())
}

/// The id of a repository the user can rename or delete.
fn require_editable(kind: &str) -> Result<&str> {
    let kind = kind.trim();
    if !toolchain::repo_id(kind) {
        return Err(anyhow!("unknown repository"));
    }
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use kiki_agentfile as agentfile;

    use super::*;
    use crate::tasks::test_support::{call, new_task_test_server, state};

    fn kinds(app: &Server) -> Vec<String> {
        state(app)["repos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|repo| repo["kind"].as_str().unwrap().to_string())
            .collect()
    }

    fn repo_file(root: &Path, id: &str) -> std::path::PathBuf {
        root.join(".kiki").join("repos").join(format!("{id}.yaml"))
    }

    #[test]
    fn repo_name_keeps_the_display_name_and_builds_a_hyphen_id() {
        assert_eq!(repo_name("  My Repo ").unwrap(), ("My Repo".into(), "my-repo".into()));
        assert_eq!(
            repo_name("Firmware-Team").unwrap(),
            ("Firmware-Team".into(), "firmware-team".into())
        );
        assert_eq!(
            repo_name("Automation").unwrap(),
            ("Automation".into(), "automation".into())
        );
        assert_eq!(
            repo_name("Automation Backend").unwrap(),
            ("Automation Backend".into(), "automation-backend".into())
        );
        assert_eq!(
            repo_name("AutomationBackend").unwrap(),
            ("AutomationBackend".into(), "automation-backend".into())
        );
        assert!(repo_name("").is_err());
        assert!(repo_name("123").is_err());
        assert!(repo_name("my/repo").is_err());
    }

    #[test]
    fn add_rename_and_delete_a_repository_name() {
        let (app, tmp) = new_task_test_server();
        let root = tmp.path();
        let (status, text) = call(&app, "POST", "/api/repos", Some(r#"{"name":"My Repo"}"#));
        assert_eq!(status, 200, "{text}");
        assert!(
            kinds(&app).contains(&"my-repo".to_string()),
            "kinds = {:?}",
            kinds(&app)
        );
        assert!(repo_file(root, "my-repo").is_file(), "repo file was not written");
        let saved = std::fs::read_to_string(repo_file(root, "my-repo")).unwrap();
        assert!(saved.contains("id: my-repo") && saved.contains("My Repo"), "{saved}");
        let shown = state(&app);
        let shown = shown["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["kind"] == "my-repo")
            .unwrap();
        assert_eq!(shown["name"], "My Repo");

        let (status, text) = call(&app, "POST", "/api/repos", Some(r#"{"name":"backend"}"#));
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("Backend"), "{text}");
        let (status, text) = call(&app, "POST", "/api/repos", Some(r#"{"name":"Backend"}"#));
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("Backend"), "{text}");
        let (status, text) = call(&app, "DELETE", "/api/repos/document", None);
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("Edit these agents before deleting document"), "{text}");

        let dir = agentfile::resolve(&root.to_string_lossy(), "").unwrap();
        let current = agentfile::find(&dir, "backend_runner").unwrap().unwrap();
        let mut next = current.clone();
        next.repos = vec![
            agentfile::Repository {
                kind: "my-repo".into(),
                description: "The extra tree.".into(),
                ..Default::default()
            },
            agentfile::Repository {
                kind: "backend".into(),
                description: "The backend tree.".into(),
                ..Default::default()
            },
        ];
        agentfile::save(&dir, "backend_runner", &next).unwrap();

        let (status, text) = call(&app, "PUT", "/api/repos/my-repo", Some(r#"{"name":"MyRepo"}"#));
        assert_eq!(status, 200, "{text}");
        assert!(kinds(&app).contains(&"my-repo".to_string()));
        let renamed_same = state(&app);
        let renamed_same = renamed_same["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["kind"] == "my-repo")
            .unwrap();
        assert_eq!(renamed_same["name"], "MyRepo");

        let (status, text) = call(&app, "PUT", "/api/repos/my-repo", Some(r#"{"name":"hardware"}"#));
        assert_eq!(status, 200, "{text}");
        assert!(kinds(&app).contains(&"my-repo".to_string()));
        assert!(!kinds(&app).contains(&"hardware".to_string()));
        assert!(repo_file(root, "my-repo").is_file());
        let renamed_name = state(&app);
        let renamed_name = renamed_name["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["kind"] == "my-repo")
            .unwrap();
        assert_eq!(renamed_name["name"], "hardware");
        let renamed = agentfile::find(&dir, "backend_runner").unwrap().unwrap();
        assert!(renamed.repos.iter().any(|repo| repo.kind == "my-repo"));
        let folder = renamed_name["folder"].as_str().unwrap().to_string();
        assert!(folder.ends_with("/my-repo"), "folder moved: {folder}");

        next.repos = vec![agentfile::Repository {
            kind: "my-repo".into(),
            description: "The extra tree.".into(),
            ..Default::default()
        }];
        agentfile::save(&dir, "backend_runner", &next).unwrap();
        let (status, text) = call(&app, "DELETE", "/api/repos/my-repo", None);
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("Edit these agents before deleting my-repo"), "{text}");
        assert!(kinds(&app).contains(&"my-repo".to_string()));

        next.repos.push(agentfile::Repository {
            kind: "backend".into(),
            description: "The backend tree.".into(),
            ..Default::default()
        });
        agentfile::save(&dir, "backend_runner", &next).unwrap();
        let (status, text) = call(&app, "DELETE", "/api/repos/my-repo", None);
        assert_eq!(status, 200, "{text}");
        assert!(!kinds(&app).contains(&"my-repo".to_string()));
        assert!(!repo_file(root, "my-repo").is_file());
        assert!(Path::new(&folder).is_dir(), "the folder on disk was removed");
        let cleared = agentfile::find(&dir, "backend_runner").unwrap().unwrap();
        assert!(cleared.repos.iter().all(|repo| repo.kind != "my-repo"));
    }

    #[test]
    fn a_built_in_repository_can_be_renamed_and_deleted() {
        let (app, tmp) = new_task_test_server();
        let root = tmp.path();
        let (status, text) = call(&app, "PUT", "/api/repos/website", Some(r#"{"name":"Web App"}"#));
        assert_eq!(status, 200, "{text}");
        assert!(
            kinds(&app).contains(&"website".to_string()),
            "kinds = {:?}",
            kinds(&app)
        );
        assert!(!kinds(&app).contains(&"web-app".to_string()));
        let shown = state(&app);
        let shown = shown["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["kind"] == "website")
            .unwrap();
        assert_eq!(shown["name"], "Web App");
        let dir = agentfile::resolve(&root.to_string_lossy(), "").unwrap();
        let runner = agentfile::find(&dir, "website_runner").unwrap().unwrap();
        assert!(runner.repos.iter().any(|repo| repo.kind == "website"));

        let (status, text) = call(&app, "DELETE", "/api/repos/document", None);
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("Edit these agents before deleting document"), "{text}");
        assert!(kinds(&app).contains(&"document".to_string()));

        for id in ["requirement_document", "requirement_designer"] {
            let current = agentfile::find(&dir, id).unwrap().unwrap();
            let mut next = current.clone();
            next.repos.push(agentfile::Repository {
                kind: "backend".into(),
                description: "Also the backend.".into(),
                ..Default::default()
            });
            agentfile::save(&dir, id, &next).unwrap();
        }
        let (status, text) = call(&app, "DELETE", "/api/repos/document", None);
        assert_eq!(status, 200, "{text}");
        assert!(!kinds(&app).contains(&"document".to_string()));
        assert!(!repo_file(root, "document").is_file());
        assert!(state(&app)["repos"]
            .as_array()
            .unwrap()
            .iter()
            .all(|repo| repo["kind"] != "document"));
    }

    #[test]
    fn the_same_id_reports_the_existing_name() {
        let (app, _tmp) = new_task_test_server();
        let (status, text) = call(&app, "POST", "/api/repos", Some(r#"{"name":"Automation Backend"}"#));
        assert_eq!(status, 200, "{text}");
        let (status, text) = call(&app, "POST", "/api/repos", Some(r#"{"name":"AutomationBackend"}"#));
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("Automation Backend"), "{text}");
        let (status, text) = call(
            &app,
            "PUT",
            "/api/repos/automation-backend",
            Some(r#"{"name":"Website"}"#),
        );
        assert_eq!(status, 200, "{text}");
        let shown = state(&app);
        let shown = shown["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["kind"] == "automation-backend")
            .unwrap();
        assert_eq!(shown["name"], "Website");
        assert!(kinds(&app).contains(&"website".to_string()));
    }
}
