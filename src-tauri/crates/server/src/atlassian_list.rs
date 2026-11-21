//! The Jira boards and Confluence spaces and pages the Settings and agent forms pick from.
//!
//! Settings can list them before Save. A POST body is the form on screen; a blank token still keeps the
//! saved token. GET, and a POST with no body, uses the saved setup. Nothing here is stored.

use anyhow::Result;
use serde_json::json;

use kiki_mcp as mcp;

use crate::api_error::{ApiResult, OrStatus};
use crate::atlassian::AtlassianBody;
use crate::controller::Controller;
use crate::http::{read_json, write_json, Request, Response, Router};
use crate::Server;

/// Pattern: Controller. The Atlassian lookup routes.
pub(crate) struct AtlassianController;

impl Controller for AtlassianController {
    fn register(router: &mut Router<Server>) {
        for method in ["GET", "POST"] {
            router.api(&format!("{method} /api/atlassian/jira/boards"), |server, req| {
                server.atlassian_list(req, |cfg| server.atlassian.boards(&req.ctx, cfg, req.query_get("q")))
            });
            router.api(&format!("{method} /api/atlassian/confluence/spaces"), |server, req| {
                server.atlassian_list(req, |cfg| server.atlassian.spaces(&req.ctx, cfg, req.query_get("q")))
            });
            router.api(&format!("{method} /api/atlassian/confluence/pages"), |server, req| {
                server.atlassian_list(req, |cfg| pages(server, req, cfg))
            });
        }
    }
}

fn pages(server: &Server, req: &Request, cfg: &mcp::Atlassian) -> Result<Vec<mcp::Hit>> {
    server.atlassian.pages(
        &req.ctx,
        cfg,
        req.query_get("space"),
        req.query_get("key"),
        req.query_get("q"),
    )
}

impl Server {
    /// Asks `load` for the items and answers `{"items": [...]}`.
    /// The setup is the form in the body when Settings has not been saved yet, otherwise the saved setup.
    fn atlassian_list(
        &self,
        req: &Request,
        load: impl FnOnce(&mcp::Atlassian) -> Result<Vec<mcp::Hit>>,
    ) -> ApiResult<Response> {
        let saved = self.store.atlassian().or_status(500)?;
        let cfg = list_config(req, &saved)?;
        let items = load(&cfg).or_status(400)?;
        Ok(write_json(200, &json!({ "items": items })))
    }
}

/// The setup a list should call with. An empty body keeps the saved setup.
/// A form is merged the same way Save merges it, so a blank token keeps the saved token, then discarded.
fn list_config(req: &Request, saved: &mcp::Atlassian) -> ApiResult<mcp::Atlassian> {
    if req.body.iter().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')) {
        return Ok(saved.clone());
    }
    let form: AtlassianBody = read_json(req)?;
    Ok(mcp::apply(saved, &form.form()))
}
