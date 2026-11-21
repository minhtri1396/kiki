//! Launching an agent. `LaunchSpec` is the parameter object, built with `LaunchBuilder` (pattern: Builder);
//! `Launcher` runs the steps for a plain launch or a feature (task worktree) launch.

use crate::backend::Backend;
use crate::groups::Groups;
use crate::projects::Projects;
use crate::terminal::{terminal_handle_bytes, Terminals};
use crate::worktrees::{same_git_repo, Worktrees};
use crate::Client;
use anyhow::{anyhow, Result};
use kiki_util::Ctx;

/// The task worktree a feature launch runs in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureSpec {
    /// The Orca sidebar folder (group) for the task.
    pub group: String,
    /// The worktree name.
    pub worktree: String,
    /// The git branch of the worktree.
    pub branch: String,
}

/// Everything one agent launch needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchSpec {
    /// The agent CLI to start (`cursor`, `claude`, ...).
    pub runtime: String,
    /// The repository folder.
    pub root: String,
    /// The first prompt; may be empty.
    pub prompt: String,
    /// The Orca project that holds the repository.
    pub project: String,
    /// The terminal tab title.
    pub tab: String,
    /// The model to ask for; empty keeps the agent's default.
    pub model: String,
    /// Set to run in a task worktree created from `root`.
    pub feature: Option<FeatureSpec>,
}

impl LaunchSpec {
    /// Starts a builder with the four required parts.
    pub fn builder(
        runtime: impl Into<String>,
        root: impl Into<String>,
        project: impl Into<String>,
        tab: impl Into<String>,
    ) -> LaunchBuilder {
        LaunchBuilder {
            spec: LaunchSpec {
                runtime: runtime.into(),
                root: root.into(),
                project: project.into(),
                tab: tab.into(),
                ..Default::default()
            },
        }
    }
}

/// Builds a [`LaunchSpec`].
#[derive(Clone, Debug)]
pub struct LaunchBuilder {
    spec: LaunchSpec,
}

impl LaunchBuilder {
    /// The first prompt.
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.spec.prompt = prompt.into();
        self
    }

    /// The model to ask for.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.spec.model = model.into();
        self
    }

    /// Runs the agent in a task worktree: the project joins `group`, and the worktree is created from `root`.
    pub fn feature(mut self, group: impl Into<String>, worktree: impl Into<String>, branch: impl Into<String>) -> Self {
        self.spec.feature = Some(FeatureSpec {
            group: group.into(),
            worktree: worktree.into(),
            branch: branch.into(),
        });
        self
    }

    /// The finished spec.
    pub fn build(self) -> LaunchSpec {
        self.spec
    }
}

/// LaunchArgs is `orca agent launch` for a terminal agent in this repository.
pub fn launch_args(runtime: &str, root: &str, prompt: &str, model: &str) -> Vec<String> {
    let mut args: Vec<String> = vec!["agent".into(), "launch".into(), "--agent".into(), runtime.into()];
    if !model.trim().is_empty() {
        args.push("--model".into());
        args.push(model.into());
    }
    args.extend([
        "--worktree".to_string(),
        format!("path:{root}"),
        "--surface".into(),
        "terminal".into(),
        "--focus".into(),
        "--prompt".into(),
        prompt.into(),
        "--json".into(),
    ]);
    args
}

/// Runs a launch step by step through the client's collaborators.
pub(crate) struct Launcher<'a> {
    client: &'a Client,
    backend: Backend,
}

impl<'a> Launcher<'a> {
    pub(crate) fn new(client: &'a Client) -> Launcher<'a> {
        Launcher {
            client,
            backend: Backend::new(client),
        }
    }

    /// Opens the repo folder in the named Orca project, starts the agent, and titles the tab. A feature launch
    /// first puts the project in the group folder and creates the task worktree from that repo.
    pub(crate) fn launch(&self, ctx: &Ctx, spec: &LaunchSpec) -> Result<String> {
        self.backend.cli.ensure_running(ctx)?;
        let folder = match &spec.feature {
            None => {
                Projects::new(self.backend.clone()).ensure(ctx, &spec.root, &spec.project)?;
                spec.root.clone()
            }
            Some(feature) => self.checkout(
                ctx,
                &spec.root,
                &spec.project,
                &feature.group,
                &feature.worktree,
                &feature.branch,
            )?,
        };
        self.client.note_worktree(&folder);
        self.start_agent(ctx, spec, &folder)
    }

    /// Opens one repository and returns the task worktree created from that git repo.
    pub(crate) fn ensure_task_worktree(
        &self,
        ctx: &Ctx,
        root: &str,
        project: &str,
        group: &str,
        name: &str,
        branch: &str,
    ) -> Result<String> {
        self.backend.cli.ensure_running(ctx)?;
        self.checkout(ctx, root, project, group, name, branch)
    }

    /// The folder of one task worktree: the project is opened, placed in `group`, and the worktree is created
    /// from `root` on `branch`.
    fn checkout(&self, ctx: &Ctx, root: &str, project: &str, group: &str, name: &str, branch: &str) -> Result<String> {
        Projects::new(self.backend.clone()).ensure(ctx, root, project)?;
        Groups::new(self.backend.clone()).ensure(ctx, group, project)?;
        let checkout = Worktrees::new(self.backend.clone()).ensure(ctx, root, project, name, branch)?;
        same_git_repo(root, &checkout)?;
        Ok(checkout)
    }

    /// Launches the agent in `folder`, titles its terminal, and makes sure the prompt is submitted.
    fn start_agent(&self, ctx: &Ctx, spec: &LaunchSpec, folder: &str) -> Result<String> {
        let cli = &self.backend.cli;
        let out = cli.run(ctx, &launch_args(&spec.runtime, folder, &spec.prompt, &spec.model))?;
        let handle = terminal_handle_bytes(&out);
        if handle.is_empty() {
            return Err(anyhow!("Orca did not return a terminal for the agent"));
        }
        cli.run(
            ctx,
            &[
                "terminal",
                "rename",
                "--terminal",
                handle.as_str(),
                "--title",
                &spec.tab,
                "--json",
            ],
        )?;
        if !spec.prompt.trim().is_empty() {
            Terminals::new(cli.clone()).submit_pasted_prompt(ctx, &handle)?;
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_optional_parts() {
        let plain = LaunchSpec::builder("cursor", "/r", "p", "tab").build();
        assert_eq!(plain.feature, None);
        assert_eq!((plain.prompt.as_str(), plain.model.as_str()), ("", ""));
        let full = LaunchSpec::builder("cursor", "/r", "p", "tab")
            .prompt("do it")
            .model("m")
            .feature("4", "12", "feat/x")
            .build();
        assert_eq!(full.prompt, "do it");
        assert_eq!(full.model, "m");
        assert_eq!(
            full.feature,
            Some(FeatureSpec {
                group: "4".into(),
                worktree: "12".into(),
                branch: "feat/x".into()
            })
        );
    }

    #[test]
    fn launch_args_include_model_only_when_set() {
        let args = launch_args("cursor", "/r", "hi", " ");
        assert!(!args.contains(&"--model".to_string()));
        assert_eq!(args[..4], ["agent", "launch", "--agent", "cursor"]);
        let args = launch_args("cursor", "/r", "hi", "m1");
        assert_eq!(args[4..6], ["--model", "m1"]);
        assert!(args.windows(2).any(|w| w == ["--worktree", "path:/r"]));
        assert_eq!(args.last().unwrap(), "--json");
    }

    fn script_client(dir: &std::path::Path, log: &std::path::Path, launch_reply: &str) -> Client {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("orca");
        let body = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
case "$1 $2" in
  "status --json") echo '{{"ok":true,"result":{{"app":{{"running":true}}}}}}' ;;
  "project list") echo '{{"ok":true,"result":{{"projects":[]}}}}' ;;
  "project setups") echo '{{"ok":true,"result":{{"setups":[]}}}}' ;;
  "repo add") echo '{{"ok":true,"result":{{"repo":{{"id":"r1","displayName":"ws-backend"}}}}}}' ;;
  "agent launch") echo '{launch_reply}' ;;
  "terminal rename") echo '{{"ok":true}}' ;;
  *) echo unexpected >&2; exit 1 ;;
esac
"#,
            log = log.display()
        );
        std::fs::write(&script, body).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Client {
            bin: script.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    const WITH_HANDLE: &str = r#"{"ok":true,"result":{"outcome":{"handle":"term_a"}}}"#;

    #[test]
    fn plain_launch_runs_the_steps_in_order_and_reports_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("args");
        let mut client = script_client(dir.path(), &log, WITH_HANDLE);
        let folders = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen = folders.clone();
        client.on_worktree = Some(std::sync::Arc::new(move |path: &str| {
            seen.lock().unwrap().push(path.to_string())
        }));
        let spec = LaunchSpec::builder("cursor", "/tmp/repo", "ws-backend", "agent-1").build();
        let out = client.launch(&Ctx::background(), &spec).unwrap();
        assert!(out.contains("term_a"), "out = {out}");
        assert_eq!(*folders.lock().unwrap(), vec!["/tmp/repo".to_string()]);
        let log = std::fs::read_to_string(&log).unwrap();
        let calls: Vec<&str> = log.lines().collect();
        let verbs: Vec<String> = calls
            .iter()
            .map(|l| l.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(
            verbs,
            [
                "status --json",
                "project list",
                "project setups",
                "repo add",
                "agent launch",
                "terminal rename"
            ],
            "calls = {calls:?}"
        );
        assert!(
            calls[5].contains("--terminal term_a --title agent-1"),
            "rename = {}",
            calls[5]
        );
    }

    #[test]
    fn launch_fails_without_a_terminal_handle() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("args");
        let client = script_client(dir.path(), &log, r#"{"ok":true,"result":{}}"#);
        let spec = LaunchSpec::builder("cursor", "/tmp/repo", "ws-backend", "t").build();
        let err = client.launch(&Ctx::background(), &spec).unwrap_err().to_string();
        assert_eq!(err, "Orca did not return a terminal for the agent");
    }
}
