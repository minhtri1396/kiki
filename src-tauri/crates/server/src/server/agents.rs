//! Agents: read, save, preview and launch one agent, and the launch history.

use std::time::Duration;

use serde_json::json;

use kiki_agentfile as agentfile;
use kiki_console as console;
use kiki_store as store;

use super::agent_form::AgentForm;
use super::agent_json::agent_json;
use super::prepare::model_tool;
use super::Server;
use crate::api_error::{ApiError, ApiResult, OrStatus};
use crate::controller::Controller;
use crate::http::{read_json, write_json, Request, Response, Router};

/// Pattern: Controller. The agent routes.
pub(crate) struct AgentController;

impl Controller for AgentController {
    fn register(router: &mut Router<Server>) {
        router.api("GET /api/agents/{id}", |server, req| {
            Ok(write_json(
                200,
                &agent_json(&server.lookup_agent(req.path_value("id"))?),
            ))
        });
        router.api("PUT /api/agents/{id}", |server, req| {
            server.save_agent(req.path_value("id"), read_json(req)?)?;
            server.state_reply()
        });
        router.api("POST /api/agents/{id}/preview", |server, req| server.preview_agent(req));
        router.api("POST /api/agents/{id}/launch", |server, req| server.launch_agent(req));
        router.api("DELETE /api/launches/{id}", |server, req| {
            server.delete_launch(req.path_value("id"))
        });
        router.api("DELETE /api/launches", |server, _| {
            server.store.clear_launches().or_status(500)?;
            Ok(write_json(200, &json!({ "ok": true })))
        });
    }
}

impl Server {
    /// Saves the agent file `id` from the edit-agent form.
    fn save_agent(&self, id: &str, form: AgentForm) -> ApiResult<()> {
        let dir = self.agent_dir().or_status(500)?;
        let tool = model_tool(&form.tool).or_status(400)?;
        let model = console::resolve_model(&self.model_catalog(), tool, form.model.trim()).or_status(400)?;
        let works_with = form.collaborators().or_status(400)?;
        let next = form.into_file(id, model, works_with);
        agentfile::save(&dir, id, &next).map_err(|err| {
            let status = if agentfile::is_not_found(&err) { 404 } else { 400 };
            ApiError::from_anyhow(status, &err)
        })
    }

    fn preview_agent(&self, req: &Request) -> ApiResult<Response> {
        let prep = self.prepare(req, true)?;
        let (command, project) = self.preview_orchestrator(&prep).or_status(400)?;
        Ok(write_json(
            200,
            &json!({
                "agentId": prep.agent.id,
                "runtime": prep.tool.id(),
                "model": prep.model,
                "command": command,
                "prompt": prep.prompt,
                "note": prep.note,
                "worktree": prep.worktree,
                "project": project,
                "tab": prep.agent.id,
            }),
        ))
    }

    /// Starts the agent and records the attempt, whether it worked or not.
    fn launch_agent(&self, req: &Request) -> ApiResult<Response> {
        let prep = self.prepare(req, false)?;
        let ctx = req.ctx.with_timeout(Duration::from_secs(120));
        let result = self.launch_orchestrator(&ctx, &prep);
        ctx.cancel();
        let (status, detail, raw) = match result {
            Ok(raw) => ("ok", raw.clone(), Some(raw)),
            Err(err) => ("error", format!("{err:#}"), None),
        };
        let launch = self
            .store
            .add_launch(
                &prep.agent.id,
                prep.tool.id(),
                &prep.note,
                status,
                &store::detail_limit(&detail),
            )
            .or_status(500)?;
        if let Some(raw) = raw {
            self.remember_task_run(&prep, &raw).or_status(500)?;
        }
        Ok(write_json(200, &json!({ "launch": launch })))
    }

    fn delete_launch(&self, raw_id: &str) -> ApiResult<Response> {
        let id = match raw_id.parse::<i64>() {
            Ok(id) if id > 0 => id,
            _ => return Err(ApiError::not_found(store::ErrNotFound)),
        };
        self.store.delete_launch(id).map_err(|err| {
            let status = if store::is_not_found(&err) { 404 } else { 500 };
            ApiError::from_anyhow(status, &err)
        })?;
        Ok(write_json(200, &json!({ "ok": true })))
    }
}
