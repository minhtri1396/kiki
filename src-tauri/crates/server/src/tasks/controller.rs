//! Pattern: Controller. The task routes: parse the request, call the service methods, shape the reply.

use std::time::Duration;

use serde_json::json;

use super::branch::TaskBranch;
use super::error::{task_error, task_id};
use crate::api_error::ApiResult;
use crate::controller::Controller;
use crate::http::{write_json, Request, Response, Router};
use crate::Server;

/// How long the Orca cleanup of a deleted task may take.
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2 * 60);

pub(crate) struct TaskController;

impl Controller for TaskController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/tasks", TaskController::create);
        router.api("PUT /api/tasks/{id}", TaskController::update);
        router.api("DELETE /api/tasks/{id}", TaskController::delete);
        router.api("POST /api/tasks/{id}/branch", TaskController::branch);
    }
}

impl TaskController {
    fn create(server: &Server, req: &Request) -> ApiResult<Response> {
        let input = server.read_task(req, "")?;
        let task = server.store.create_task(input).map_err(|err| task_error(&err))?;
        Ok(write_json(201, &json!({ "task": task })))
    }

    fn update(server: &Server, req: &Request) -> ApiResult<Response> {
        let id = task_id(req)?;
        let input = server.read_task(req, &id)?;
        let task = server.store.update_task(&id, input).map_err(|err| task_error(&err))?;
        Ok(write_json(200, &json!({ "task": task })))
    }

    fn delete(server: &Server, req: &Request) -> ApiResult<Response> {
        let id = task_id(req)?;
        server.delete_task(&req.ctx.with_timeout(CLEANUP_TIMEOUT), &id)?;
        Ok(write_json(200, &json!({ "ok": true })))
    }

    fn branch(server: &Server, req: &Request) -> ApiResult<Response> {
        let id = task_id(req)?;
        let TaskBranch { task_id, slug, ticket } = server.task_branch(&req.ctx, &id)?;
        Ok(write_json(
            200,
            &json!({ "taskId": task_id, "slug": slug, "ticket": ticket }),
        ))
    }
}
