//! Pattern: Controller. `POST /api/repos/{kind}/clone`: read the form, clone, answer with the new state.

use kiki_toolchain as toolchain;
use serde::Deserialize;

use crate::api_error::{ApiError, ApiResult};
use crate::controller::Controller;
use crate::http::{read_json, Request, Response, Router};
use crate::tasks::null_default;
use crate::Server;

pub(crate) struct CloneController;

impl Controller for CloneController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/repos/{kind}/clone", CloneController::clone_repo);
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CloneBody {
    #[serde(deserialize_with = "null_default")]
    url: String,
    #[serde(deserialize_with = "null_default")]
    sparse: String,
}

impl CloneController {
    fn clone_repo(server: &Server, req: &Request) -> ApiResult<Response> {
        let kind = req.path_value("kind");
        if !toolchain::repo_id(kind) {
            return Err(ApiError::new(404, "unknown repo"));
        }
        let body: CloneBody = read_json(req)?;
        server.clone_repository(&req.ctx, kind, &body.url, &body.sparse)?;
        server.state_reply()
    }
}
