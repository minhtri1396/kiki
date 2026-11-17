//! Pattern: Controller. The group routes: create and delete.

use serde::Deserialize;
use serde_json::json;

use crate::api_error::{ApiError, ApiResult};
use crate::controller::Controller;
use crate::http::{read_json, write_json, Request, Response, Router, MAX_JSON_BODY};
use crate::tasks::null_default;
use crate::Server;

pub(crate) struct GroupController;

impl Controller for GroupController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/groups", GroupController::create);
        router.api("DELETE /api/groups/{id}", GroupController::delete);
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CreateBody {
    #[serde(deserialize_with = "null_default")]
    name: String,
    #[serde(deserialize_with = "null_default")]
    description: String,
}

/// The delete body is optional: `{"jira": true}` also deletes the group's epic.
#[derive(Deserialize, Default)]
#[serde(default)]
struct DeleteBody {
    jira: bool,
}

impl DeleteBody {
    /// An empty body means "keep Jira"; otherwise the first JSON value is read and anything after it is ignored.
    fn read(req: &Request) -> ApiResult<DeleteBody> {
        let invalid = || ApiError::new(400, "invalid data");
        if req.body.len() > MAX_JSON_BODY {
            return Err(invalid());
        }
        if req.body.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Ok(DeleteBody::default());
        }
        let mut stream = serde_json::Deserializer::from_slice(&req.body);
        DeleteBody::deserialize(&mut stream).map_err(|_| invalid())
    }
}

impl GroupController {
    fn create(server: &Server, req: &Request) -> ApiResult<Response> {
        let body: CreateBody = read_json(req)?;
        let group = server.create_group(&req.ctx, &body.name, &body.description)?;
        Ok(write_json(201, &json!({ "group": group })))
    }

    fn delete(server: &Server, req: &Request) -> ApiResult<Response> {
        let body = DeleteBody::read(req)?;
        server.delete_group(&req.ctx, req.path_value("id"), body.jira)?;
        Ok(write_json(200, &json!({ "ok": true })))
    }
}
