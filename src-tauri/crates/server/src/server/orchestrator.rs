//! Starting an agent in the Agent Orchestration tool Settings chose. Only Orca is supported.
//!
//! Pattern: State. [`Orchestrator`] is the closed set of tools; each launch and preview `match`es it exhaustively.

use anyhow::{anyhow, Result};

use kiki_console as console;
use kiki_jobs as jobs;
use kiki_orca as orca;
use kiki_orca::LaunchSpec;
use kiki_util::Ctx;

use super::prepare::LaunchPrep;
use super::worktree::repo_readonly;
use super::Server;

/// An Agent Orchestration tool that can start agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orchestrator {
    Orca,
}

impl Orchestrator {
    /// The tool saved in the setup, or Orca when none is saved.
    fn of(prep: &LaunchPrep) -> Result<Orchestrator> {
        match console::normalize_orchestrator(&prep.snap.orchestrator).as_str() {
            "orca" => Ok(Orchestrator::Orca),
            _ => Err(anyhow!("Agent Orchestration tool is not supported")),
        }
    }
}

impl Server {
    /// The command that would start this agent, and the Orca project it would run in.
    pub(crate) fn preview_orchestrator(&self, prep: &LaunchPrep) -> Result<(String, String)> {
        match Orchestrator::of(prep)? {
            Orchestrator::Orca => preview_orca(prep),
        }
    }

    /// Prepares guardrails, hooks, tools and MCP servers for the agent, then starts it.
    pub(crate) fn launch_orchestrator(&self, ctx: &Ctx, prep: &LaunchPrep) -> Result<String> {
        self.guard_binder().bind(
            &prep.agent.id,
            &prep.worktree,
            repo_readonly(&prep.agent.repos, &prep.kind),
            &prep.snap.repo(&prep.kind).services,
        );
        self.ensure_agent_hooks(prep.tool)?;
        self.ensure_agent_tools(prep)?;
        self.ensure_agent_mcps(prep)?;
        match Orchestrator::of(prep)? {
            Orchestrator::Orca => self.launch_orca(ctx, prep),
        }
    }

    fn launch_orca(&self, ctx: &Ctx, prep: &LaunchPrep) -> Result<String> {
        let mut client = self.orca.clone();
        client.env.push(format!("KIKI_AGENT={}", prep.agent.id));
        // The callback outlives this call's borrow of `self`, so it holds only the shared guard state.
        let binder = self.guard_binder();
        let agent_id = prep.agent.id.clone();
        let readonly = repo_readonly(&prep.agent.repos, &prep.kind);
        let services = prep.snap.repo(&prep.kind).services.clone();
        // Each other repository is its own git repo, so it gets its own task worktree before the agent starts.
        if !prep.task_id.is_empty() {
            for repo in task_repos(prep)? {
                if repo.kind == prep.kind {
                    continue;
                }
                let checkout = client.ensure_task_worktree(
                    ctx,
                    &repo.folder,
                    &repo.project,
                    &prep.group_id,
                    &prep.task_id,
                    &repo.branch,
                )?;
                binder.bind(&prep.agent.id, &checkout, repo.readonly, &repo.services);
            }
        }
        client.on_worktree = Some(std::sync::Arc::new(move |path: &str| {
            binder.bind(&agent_id, path, readonly, &services);
        }));
        client.launch(ctx, &launch_spec(prep)?)
    }
}

/// The Orca launch for `prep`: a plain launch, or a task worktree when the launch belongs to a task.
fn launch_spec(prep: &LaunchPrep) -> Result<LaunchSpec> {
    let builder = |project: String| {
        LaunchSpec::builder(prep.tool.id(), &prep.worktree, project, &prep.agent.id)
            .prompt(&prep.prompt)
            .model(&prep.model)
    };
    let project = orca::project_name(&prep.snap.workspace_id, &prep.kind)?;
    if prep.task_id.is_empty() {
        return Ok(builder(project).build());
    }
    if prep.group_id.trim().is_empty() {
        return Err(anyhow!("Choose a group before starting this task."));
    }
    let project = orca::feature_project_name(&prep.snap.workspace_id, &prep.group_id, &prep.kind)?;
    let branch = orca::feature_branch(&prep.ticket, &prep.slug, &prep.kind)?;
    Ok(builder(project).feature(&prep.group_id, &prep.task_id, branch).build())
}

fn preview_orca(prep: &LaunchPrep) -> Result<(String, String)> {
    let project = orca::project_name(&prep.snap.workspace_id, &prep.kind)?;
    if prep.task_id.is_empty() {
        return Ok((orca_command(prep, &project), project));
    }
    if prep.group_id.trim().is_empty() {
        return Err(anyhow!("Choose a group before starting this task."));
    }
    let project = orca::feature_project_name(&prep.snap.workspace_id, &prep.group_id, &prep.kind)?;
    let mut command = format!(
        "folder {}\n{}\nworktree {}",
        prep.group_id,
        orca_command(prep, &project),
        prep.task_id
    );
    for kind in other_repo_kinds(prep) {
        command.push_str(&format!("\nworktree {} {kind}", prep.task_id));
    }
    Ok((command, project))
}

/// Repository kinds on this agent, other than the one the agent process starts in, that have a folder.
fn other_repo_kinds(prep: &LaunchPrep) -> Vec<String> {
    prep.agent
        .repos
        .iter()
        .filter_map(|repo| {
            let kind = repo.kind.trim();
            if kind.is_empty() || kind == prep.kind {
                return None;
            }
            let saved = prep.snap.repo(kind);
            let folder = saved.folder.trim();
            if folder.is_empty() {
                None
            } else {
                Some(kind.to_string())
            }
        })
        .collect()
}

/// One repository that gets its own task worktree. The agent still starts in `prep.kind`.
struct TaskRepo {
    kind: String,
    folder: String,
    project: String,
    branch: String,
    readonly: bool,
    services: Vec<String>,
}

/// Every repository on the agent that has a folder. Each one is a separate git checkout.
fn task_repos(prep: &LaunchPrep) -> Result<Vec<TaskRepo>> {
    let mut out = Vec::new();
    for repo in &prep.agent.repos {
        let kind = repo.kind.trim();
        if kind.is_empty() {
            continue;
        }
        let saved = prep.snap.repo(kind);
        let folder = saved.folder.trim();
        if folder.is_empty() {
            continue;
        }
        out.push(TaskRepo {
            kind: kind.to_string(),
            folder: folder.to_string(),
            project: orca::feature_project_name(&prep.snap.workspace_id, &prep.group_id, kind)?,
            branch: orca::feature_branch(&prep.ticket, &prep.slug, kind)?,
            readonly: repo.readonly,
            services: saved.services.clone(),
        });
    }
    Ok(out)
}

/// The `orca agent launch` command as shown in the preview; the prompt is replaced by `<prompt>`.
fn orca_command(prep: &LaunchPrep, project: &str) -> String {
    let mut args = orca::launch_args(prep.tool.id(), &prep.worktree, &prep.prompt, &prep.model);
    for i in 0..args.len().saturating_sub(1) {
        if args[i] == "--prompt" {
            args[i + 1] = "<prompt>".to_string();
        }
    }
    format!(
        "project {project}\ntab {}\norca {}",
        prep.agent.id,
        jobs::format_args(&args)
    )
}
