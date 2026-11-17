//! Pattern: Controller. The team folder routes: parse the request, call the service method, shape the reply.

use serde::Deserialize;

use super::import::RepoDecisions;
use super::service::{MemberRequest, SaveRequest};
use super::Outcome;
use crate::api_error::ApiResult;
use crate::controller::Controller;
use crate::http::{read_json, write_json, Request, Response, Router};
use crate::Server;

pub(crate) struct TeamController;

impl Controller for TeamController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/team/browse", |s, _| Ok(write_json(200, &s.team_browse()?)));
        router.api("PUT /api/team", TeamController::save);
        router.api("GET /api/team/members", |s, r| {
            Ok(write_json(200, &s.team_members(r.query_get("path"))?))
        });
        router.api("GET /api/team/compare", |s, r| {
            Ok(write_json(200, &s.team_compare(r.query_get("member"))?))
        });
        router.api("GET /api/team/diff", TeamController::diff);
        router.api("POST /api/team/member", TeamController::member);
        router.api("POST /api/team/agents", |s, r| {
            s.save_user_agents()?;
            Ok(s.handle_state(r))
        });
        router.api("POST /api/team/repos", |s, r| {
            s.save_user_repos()?;
            Ok(s.handle_state(r))
        });
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SaveBody {
    path: String,
    user: String,
    direction: String,
    #[serde(rename = "useExisting")]
    use_existing: bool,
    repos: Option<RepoDecisions>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MemberBody {
    name: String,
    direction: String,
    part: String,
    repos: Option<RepoDecisions>,
}

impl TeamController {
    fn save(server: &Server, req: &Request) -> ApiResult<Response> {
        let body: SaveBody = read_json(req)?;
        let outcome = server.team_save(SaveRequest {
            path: body.path,
            user: body.user,
            direction: body.direction,
            use_existing: body.use_existing,
            repos: body.repos.unwrap_or_default(),
        })?;
        Ok(reply(server, req, outcome))
    }

    fn diff(server: &Server, req: &Request) -> ApiResult<Response> {
        let diff = server.team_diff(req.query_get("folder"), req.query_get("id"), req.query_get("part"))?;
        Ok(write_json(200, &diff))
    }

    fn member(server: &Server, req: &Request) -> ApiResult<Response> {
        let body: MemberBody = read_json(req)?;
        let outcome = server.team_member(MemberRequest {
            name: body.name,
            direction: body.direction,
            part: body.part,
            repos: body.repos.unwrap_or_default(),
        })?;
        Ok(reply(server, req, outcome))
    }
}

/// A finished operation answers with the new state; one that needs a decision answers 409 with what to decide.
fn reply(server: &Server, req: &Request, outcome: Outcome) -> Response {
    match outcome {
        Outcome::Done => server.handle_state(req),
        Outcome::Conflict(payload) => write_json(409, &payload),
    }
}
