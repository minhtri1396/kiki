//! Background jobs (index setup): list, read, and cancel.

use serde_json::json;

use super::Server;
use crate::api_error::{ApiError, ApiResult};
use crate::controller::Controller;
use crate::http::{write_json, Response, Router};

/// Pattern: Controller. The job routes.
pub(crate) struct JobController;

impl Controller for JobController {
    fn register(router: &mut Router<Server>) {
        router.api("GET /api/jobs", |server, _| {
            Ok(write_json(200, &json!({ "jobs": server.jobs.list() })))
        });
        router.api("GET /api/jobs/{id}", |server, req| {
            server.job_reply(req.path_value("id"))
        });
        router.api("POST /api/jobs/{id}/cancel", |server, req| {
            let id = req.path_value("id");
            if !server.jobs.cancel(id) {
                return Err(ApiError::not_found("command is no longer running"));
            }
            server.job_reply(id)
        });
    }
}

impl Server {
    fn job_reply(&self, id: &str) -> ApiResult<Response> {
        match self.jobs.get(id) {
            Some(job) => Ok(write_json(200, &job)),
            None => Err(ApiError::not_found("unknown command")),
        }
    }
}
