//! Pattern: Controller. `POST /api/guardrail`: the hooks of the AI tools ask whether an action is allowed.

use serde::Deserialize;
use serde_json::json;

use crate::api_error::ApiResult;
use crate::controller::Controller;
use crate::http::{read_json, write_json, Request, Response, Router};
use crate::tasks::null_default;
use crate::Server;

pub(crate) struct GuardController;

impl Controller for GuardController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/guardrail", GuardController::decide);
    }
}

/// What a hook sends. `input` is the raw payload of the AI tool, passed on as the JSON value that arrived.
#[derive(Deserialize, Default)]
#[serde(default)]
struct HookBody {
    #[serde(deserialize_with = "null_default")]
    agent: String,
    #[serde(deserialize_with = "null_default")]
    cwd: String,
    #[serde(deserialize_with = "null_default")]
    event: String,
    input: Option<serde_json::Value>,
}

impl GuardController {
    fn decide(server: &Server, req: &Request) -> ApiResult<Response> {
        let body: HookBody = read_json(req)?;
        let input = match &body.input {
            Some(value) => serde_json::to_vec(value).unwrap_or_default(),
            None => Vec::new(),
        };
        let decision = server.check_guard(&body.agent, &body.cwd, &body.event, &input);
        Ok(write_json(
            200,
            &json!({ "allow": decision.allow, "message": decision.message }),
        ))
    }
}
