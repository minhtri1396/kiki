//! Pattern: Controller. The run routes: sync the terminals, reply to an agent, approve an agent.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::api_error::{ApiResult, OrStatus};
use crate::controller::Controller;
use crate::http::{read_json, write_json, Request, Response, Router};
use crate::tasks::{null_default, task_id};
use crate::Server;

/// How long one sync of all terminals may take.
const SYNC_TIMEOUT: Duration = Duration::from_secs(25);

pub(crate) struct RunController;

impl Controller for RunController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/tasks/sync", RunController::sync);
        router.api("POST /api/tasks/{id}/agents/{agentId}/reply", |s, r| {
            RunController::reply(s, r)
        });
        router.api("POST /api/tasks/{id}/agents/{agentId}/approve", |s, r| {
            RunController::approve(s, r)
        });
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ReplyBody {
    #[serde(deserialize_with = "null_default")]
    text: String,
}

impl RunController {
    fn sync(server: &Server, req: &Request) -> ApiResult<Response> {
        let tasks = server
            .sync_task_runs(&req.ctx.with_timeout(SYNC_TIMEOUT))
            .or_status(500)?;
        Ok(write_json(200, &json!({ "tasks": tasks })))
    }

    fn reply(server: &Server, req: &Request) -> ApiResult<Response> {
        let id = task_id(req)?;
        let agent_id = req.path_value("agentId").trim();
        let body: ReplyBody = read_json(req)?;
        let task = server.reply_to_agent(&req.ctx, &id, agent_id, &body.text)?;
        Ok(write_json(200, &json!({ "task": task })))
    }

    fn approve(server: &Server, req: &Request) -> ApiResult<Response> {
        let id = task_id(req)?;
        let task = server.approve_task_agent(&id, req.path_value("agentId").trim())?;
        Ok(write_json(200, &json!({ "task": task })))
    }
}
