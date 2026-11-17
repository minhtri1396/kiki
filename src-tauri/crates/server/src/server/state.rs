//! `GET /api/state`: the one document the web UI renders from, and the reply every change answers with.

use anyhow::Result;
use serde_json::{json, Value};

use kiki_agentfile as agentfile;
use kiki_console as console;

use super::agent_json::agent_row;
use super::catalog::{mcps_from_atlassian, named_json, tool_choice_json, tools_from};
use super::models::model_json;
use super::state_rows::{index_rows, repo_rows};
use super::Server;
use crate::api_error::{ApiResult, OrStatus};
use crate::controller::Controller;
use crate::http::{write_json, Request, Response, Router};

/// Pattern: Controller. The state route.
pub(crate) struct StateController;

impl Controller for StateController {
    fn register(router: &mut Router<Server>) {
        router.api("GET /api/state", |server, _| server.state_reply());
    }
}

impl Server {
    /// The state document as a 200 reply.
    pub(crate) fn state_reply(&self) -> ApiResult<Response> {
        Ok(write_json(200, &self.snapshot().or_status(500)?))
    }

    /// Same as [`Server::state_reply`] for handlers that still answer with a plain `Response`.
    pub(crate) fn handle_state(&self, _req: &Request) -> Response {
        crate::api_error::respond(self.state_reply())
    }

    /// The `/api/state` document.
    pub(crate) fn snapshot(&self) -> Result<Value> {
        let snap = self.load()?;
        let launches = self.store.launches(50)?;
        let tasks = self.store.tasks()?;
        let groups = self.store.groups()?;
        let agent_path = agentfile::resolve(&self.root, "")?;
        let agents: Vec<Value> = agentfile::list(&agent_path)?.iter().map(agent_row).collect();
        let team = self.team_status()?;
        let atlassian = self.store.atlassian()?;
        let saved_tools = self.load_tools()?;
        Ok(json!({
            "root": self.root,
            "setupFile": self.store.file(),
            "agentPath": agent_path,
            "workspaceId": snap.workspace_id,
            "orchestrator": snap.orchestrator,
            "orchestrators": &*console::ORCHESTRATORS,
            "generateTool": snap.generate_tool,
            "generateModel": snap.generate_model,
            "orcaAgents": console::ORCA_AGENTS,
            "installedAgents": console::installed_agents(),
            "models": model_json(&self.model_catalog()),
            "repos": repo_rows(&snap),
            "agents": agents,
            "indexes": index_rows(&snap, &self.jobs.last()),
            "jobs": self.jobs.list(),
            "launches": launches,
            "tasks": tasks,
            "groups": groups,
            "teamFolder": team,
            "atlassian": crate::atlassian::atlassian_state(&atlassian),
            "mcps": named_json(&mcps_from_atlassian(&atlassian)),
            "tools": named_json(&tools_from(&saved_tools)),
            "toolChoices": tool_choice_json(&saved_tools),
            "guardrails": saved_tools.guardrails_on(),
        }))
    }
}
