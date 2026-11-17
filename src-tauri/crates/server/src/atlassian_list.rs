//! The Jira boards and Confluence spaces and pages the Settings and agent forms pick from.

use anyhow::Result;
use serde_json::json;

use kiki_mcp as mcp;

use crate::api_error::{ApiResult, OrStatus};
use crate::controller::Controller;
use crate::http::{write_json, Request, Response, Router};
use crate::Server;

/// Pattern: Controller. The Atlassian lookup routes.
pub(crate) struct AtlassianController;

impl Controller for AtlassianController {
    fn register(router: &mut Router<Server>) {
        router.api("GET /api/atlassian/jira/boards", |server, req| {
            server.atlassian_list(|cfg| server.atlassian.boards(&req.ctx, cfg, req.query_get("q")))
        });
        router.api("GET /api/atlassian/confluence/spaces", |server, req| {
            server.atlassian_list(|cfg| server.atlassian.spaces(&req.ctx, cfg, req.query_get("q")))
        });
        router.api("GET /api/atlassian/confluence/pages", |server, req| {
            server.atlassian_list(|cfg| pages(server, req, cfg))
        });
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
    /// Loads the saved Atlassian setup, asks `load` for the items, and answers `{"items": [...]}`.
    fn atlassian_list(&self, load: impl FnOnce(&mcp::Atlassian) -> Result<Vec<mcp::Hit>>) -> ApiResult<Response> {
        let cfg = self.store.atlassian().or_status(500)?;
        let items = load(&cfg).or_status(400)?;
        Ok(write_json(200, &json!({ "items": items })))
    }
}
