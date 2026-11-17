//! Works out one launch or preview: the agent, its AI tool and model, the repository folder, and the prompt.
//!
//! Pattern: Builder. [`LaunchRequest`] is the parsed body of the call and [`LaunchPrep`] is what the orchestrators
//! need; `Server::prepare` assembles one from the other in small named steps.

use anyhow::{anyhow, Result};
use serde::Deserialize;

use kiki_agentdef as agentdef;
use kiki_agentfile as agentfile;
use kiki_aitool::AiTool;
use kiki_console as console;
use kiki_mcp as mcp;
use kiki_orca as orca;
use kiki_util::path;

use super::agent_json::{catalog_peers, catalog_repos, launch_peers};
use super::catalog::{
    apply_atlassian_places, catalog_uses, mcps_from_atlassian, tool_prompt_uses, tools_ready, NamedId,
};
use super::worktree::worktree_of;
use super::Server;
use crate::api_error::{ApiError, ApiResult, OrStatus};
use crate::http::{null_default, read_optional_json, Request};

/// What `prepare` works out for one launch or preview.
#[derive(Debug, Clone)]
pub struct LaunchPrep {
    pub agent: agentdef::Agent,
    pub prompt: String,
    pub snap: console::Snapshot,
    pub note: String,
    pub kind: String,
    pub worktree: String,
    pub tool: AiTool,
    pub model: String,
    pub slug: String,
    pub task_id: String,
    pub group_id: String,
    pub ticket: String,
}

impl Default for LaunchPrep {
    /// An empty launch on the first AI tool; callers fill in what they need.
    fn default() -> LaunchPrep {
        LaunchPrep {
            agent: agentdef::Agent::default(),
            prompt: String::new(),
            snap: console::Snapshot::default(),
            note: String::new(),
            kind: String::new(),
            worktree: String::new(),
            tool: AiTool::Cursor,
            model: String::new(),
            slug: String::new(),
            task_id: String::new(),
            group_id: String::new(),
            ticket: String::new(),
        }
    }
}

/// What the launch and preview calls send. `wait_for` lists the agents that asked for this one. When it is set,
/// this agent is a supporting agent that waits for a request before it works.
#[derive(Debug, Clone, Default)]
pub(crate) struct LaunchRequest {
    pub note: String,
    pub repo: String,
    pub tool: String,
    pub model: String,
    pub slug: String,
    pub task_id: String,
    pub ticket: String,
    pub note_final: bool,
    pub wait_for: Vec<String>,
}

/// The JSON body of a launch or preview call.
#[derive(Deserialize, Default)]
#[serde(default)]
struct LaunchBody {
    note: String,
    repo: String,
    agent: String,
    model: String,
    slug: String,
    #[serde(rename = "taskId")]
    task_id: String,
    ticket: String,
    #[serde(rename = "noteFinal")]
    note_final: bool,
    #[serde(rename = "waitFor", deserialize_with = "null_default")]
    wait_for: Vec<String>,
}

impl LaunchRequest {
    const NOTE_LIMIT: usize = 4000;
    const FINAL_NOTE_LIMIT: usize = 8000;

    /// Reads and checks the body. An empty body is an empty request.
    pub(crate) fn from_request(req: &Request) -> Result<LaunchRequest> {
        let body: LaunchBody = read_optional_json(req).map_err(|err| anyhow!("{}", err.message()))?;
        let limit = if body.note_final {
            Self::FINAL_NOTE_LIMIT
        } else {
            Self::NOTE_LIMIT
        };
        if body.note.len() > limit {
            return Err(anyhow!("message is longer than {limit} characters"));
        }
        let slug = body.slug.trim().to_string();
        let task_id = body.task_id.trim().to_string();
        let ticket = body.ticket.trim().to_uppercase();
        let invalid = (!task_id.is_empty() && !orca::valid_task_id(&task_id))
            || (!slug.is_empty() && !orca::valid_branch_slug(&slug))
            || !orca::valid_ticket(&ticket)
            || (task_id.is_empty() && (!slug.is_empty() || !ticket.is_empty()));
        if invalid {
            return Err(anyhow!("feature name is not valid"));
        }
        Ok(LaunchRequest {
            note: body.note,
            repo: body.repo.trim().to_string(),
            tool: body.agent.trim().to_string(),
            model: body.model.trim().to_string(),
            slug,
            task_id,
            ticket,
            note_final: body.note_final,
            wait_for: body.wait_for,
        })
    }
}

/// The AI tool named by `id`; an unknown id has no models, so it fails like a missing model.
pub(super) fn model_tool(id: &str) -> Result<AiTool> {
    AiTool::parse(id.trim()).ok_or_else(|| anyhow!("model is not supported"))
}

impl Server {
    /// Resolves the agent, the AI tool, the repository folder and the prompt for a launch or preview.
    pub(crate) fn prepare(&self, req: &Request, allow_off: bool) -> ApiResult<LaunchPrep> {
        let file = self.lookup_agent(req.path_value("id"))?;
        if !file.enabled && !allow_off {
            return Err(ApiError::bad_request("that agent is turned off"));
        }
        let cfg = self.atlassian_setup().or_status(500)?;
        let enabled = self.enabled_tools().or_status(500)?;
        let agent = agent_for_prompt(&file, &cfg, &enabled);
        let launch = LaunchRequest::from_request(req).or_status(400)?;
        let waiting = self.waiting_for(&file.id, &launch.wait_for).or_status(500)?;
        let mut snap = self.load().or_status(500)?;
        let (tool, model) = self.launch_tool(&file, &launch)?;
        let (kind, worktree) = worktree_of(&agent, &snap, &launch.repo, allow_off).or_status(400)?;
        let (note, group_id) = self.task_note(&launch, &agent)?;
        if !allow_off {
            tools_ready(&crate::guard::use_ids(&file.tools), &enabled).or_status(400)?;
        }
        snap.worktree = worktree.clone();
        let prompt = console::prompt_waiting(
            &agent,
            &file.prompt,
            &snap,
            &note,
            &launch_peers(&file.works_with),
            &waiting,
        );
        Ok(LaunchPrep {
            prompt,
            agent,
            snap,
            note,
            kind,
            worktree,
            tool,
            model,
            slug: launch.slug,
            task_id: launch.task_id,
            group_id,
            ticket: launch.ticket,
        })
    }

    /// The AI tool and model for this launch: the call's choice, else the agent file's.
    fn launch_tool(&self, file: &agentfile::File, launch: &LaunchRequest) -> ApiResult<(AiTool, String)> {
        let id = if launch.tool.is_empty() {
            file.ai_tool.trim()
        } else {
            launch.tool.as_str()
        };
        if id.is_empty() {
            return Err(ApiError::bad_request("choose an AI tool"));
        }
        let tool = model_tool(id).or_status(400)?;
        let model = console::resolve_model(&self.model_catalog(), tool, &launch.model).or_status(400)?;
        Ok((tool, model))
    }

    /// The note for the prompt and the group of the linked task. A task run needs a group; unless the note is
    /// final, the task's Jira epic is added to it.
    fn task_note(&self, launch: &LaunchRequest, agent: &agentdef::Agent) -> ApiResult<(String, String)> {
        if launch.task_id.is_empty() {
            return Ok((launch.note.clone(), String::new()));
        }
        let task = self
            .store
            .task(&launch.task_id)
            .map_err(|e| crate::tasks::task_error(&e))?;
        let group_id = task.group_id.trim().to_string();
        if group_id.is_empty() {
            return Err(ApiError::bad_request("Choose a group before starting this task."));
        }
        let note = if launch.note_final {
            launch.note.clone()
        } else {
            crate::epic::epic_note(&launch.note, &task, agent)
        };
        Ok((note, group_id))
    }

    /// The agents that asked for this one: they exist, are not this agent, and are listed once.
    pub(crate) fn waiting_for(&self, own_id: &str, ids: &[String]) -> Result<Vec<console::Requester>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let dir = self.agent_dir()?;
        let mut out: Vec<console::Requester> = Vec::new();
        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::from([own_id]);
        for id in ids.iter().map(|id| id.trim()) {
            if id.is_empty() || !seen.insert(id) {
                continue;
            }
            if let Some(file) = agentfile::find(&dir, id)? {
                out.push(console::Requester {
                    id: file.id,
                    title: file.name,
                });
            }
        }
        Ok(out)
    }

    /// The agent file for `id`: 404 when there is none, 500 when the folder cannot be read.
    pub(crate) fn lookup_agent(&self, id: &str) -> ApiResult<agentfile::File> {
        let dir = self.agent_dir().or_status(500)?;
        match agentfile::find(&dir, id).or_status(500)? {
            Some(file) => Ok(file),
            None => Err(ApiError::not_found(agentfile::ErrNotFound)),
        }
    }
}

/// The agent as the prompt builder sees it: Settings' MCP servers and tools applied to the agent's own picks.
fn agent_for_prompt(file: &agentfile::File, cfg: &mcp::Atlassian, enabled: &[NamedId]) -> agentdef::Agent {
    let mcps = mcps_from_atlassian(cfg);
    let file_name = if file.category.is_empty() {
        format!("{}.md", file.id)
    } else {
        path::clean(&format!("{}/{}.md", file.category, file.id))
    };
    agentdef::Agent {
        id: file.id.clone(),
        title: file.name.clone(),
        group: file.category.clone(),
        file: file_name,
        repos: catalog_repos(&file.repos),
        peers: catalog_peers(&file.works_with),
        mcps: apply_atlassian_places(catalog_uses(&file.mcps, &mcps), cfg),
        tools: tool_prompt_uses(&file.tools, enabled),
        summary: file.description.clone(),
    }
}
