//! Repository indexes (Obsidian, CodeGraph): checks, choosing and clearing an index, and starting its setup.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use kiki_console as console;
use kiki_jobs as jobs;
use kiki_store as store;
use kiki_toolchain as toolchain;

use super::index_tool::{sole_index_tool, IndexTool};
use super::settings::known_repo;
use super::Server;
use crate::api_error::{ApiError, ApiResult, OrStatus};
use crate::controller::Controller;
use crate::http::{read_json, read_optional_json, write_json, Request, Response, Router};

/// Pattern: Controller. The index routes, and the routes that open pages in the browser.
pub(crate) struct IndexController;

impl Controller for IndexController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/indexes/check", |server, req| server.check_indexes(req));
        router.api("POST /api/indexes/check/{kind}", |server, req| server.check_index(req));
        router.api("POST /api/indexes/refresh/{index}", |server, req| {
            let snap = server.load().or_status(500)?;
            server
                .jobs
                .refresh_folders(&targets_of(&snap), req.path_value("index").trim())
                .or_status(400)?;
            server.state_reply()
        });
        router.api("DELETE /api/indexes/{kind}", |server, req| server.clear_index(req));
        router.api("PUT /api/indexes/{kind}", |server, req| server.use_index(req));
        router.api("POST /api/indexes/obsidian", |server, req| {
            server.begin_index(req, IndexTool::Obsidian)
        });
        router.api("POST /api/indexes/codegraph", |server, req| {
            server.begin_index(req, IndexTool::Codegraph)
        });
        router.api("POST /api/indexes/codegraph/{kind}", |server, req| {
            server.begin_index(req, IndexTool::Codegraph)
        });
        router.api("POST /api/open", |server, req| {
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct Body {
                url: String,
            }
            let body: Body = serde_json::from_slice(&req.body).map_err(|_| ApiError::bad_request("invalid data"))?;
            let page = crate::open::index_page(&body.url).or_status(400)?;
            (server.browser)(page).or_status(400)?;
            Ok(write_json(200, &json!({ "ok": true })))
        });
        router.api("POST /api/jira/open", |server, req| server.open_jira_ticket(req));
    }
}

/// The repositories as the index jobs see them.
fn targets_of(snap: &console::Snapshot) -> Vec<jobs::Target> {
    snap.repos
        .iter()
        .map(|repo| jobs::Target {
            kind: repo.kind.clone(),
            folder: repo.folder.clone(),
            index: repo.index.clone(),
        })
        .collect()
}

impl Server {
    /// Checks every repository's index, remembering the index a folder already has.
    fn check_indexes(&self, req: &Request) -> ApiResult<Response> {
        let ctx = req.ctx.with_timeout(Duration::from_secs(45));
        let result = self.check_all(&ctx);
        ctx.cancel();
        result?;
        self.state_reply()
    }

    fn check_all(&self, ctx: &kiki_util::Ctx) -> ApiResult<()> {
        let snap = self.load().or_status(500)?;
        let snap = self.remember_present_indexes(snap).or_status(500)?;
        self.jobs.check(ctx, &self.root, &targets_of(&snap)).or_status(500)?;
        Ok(())
    }

    fn check_index(&self, req: &Request) -> ApiResult<Response> {
        let snap = self.load().or_status(500)?;
        let ctx = req.ctx.with_timeout(Duration::from_secs(45));
        let result = self
            .jobs
            .check_one(&ctx, &self.root, &targets_of(&snap), req.path_value("kind"));
        ctx.cancel();
        result.or_status(400)?;
        self.state_reply()
    }

    /// Forgets the repository's index, and deletes its folder when asked to.
    fn clear_index(&self, req: &Request) -> ApiResult<Response> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Body {
            #[serde(rename = "deleteFolder")]
            delete_folder: bool,
        }
        let kind = known_repo(req.path_value("kind"))?;
        let body: Body = read_optional_json(req)?;
        let snap = self.load().or_status(500)?;
        let repo = snap.repo(&kind);
        if body.delete_folder {
            store::remove_index_folder(&repo.folder, &repo.index).or_status(400)?;
        }
        self.store.save_repo_index(&snap, &kind, "").or_status(400)?;
        self.state_reply()
    }

    /// Chooses an index tool the repository already has.
    fn use_index(&self, req: &Request) -> ApiResult<Response> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Body {
            index: String,
        }
        let kind = known_repo(req.path_value("kind"))?;
        let body: Body = read_json(req)?;
        let tool = IndexTool::parse(body.index.trim())
            .ok_or_else(|| ApiError::bad_request("index must be obsidian or codegraph"))?;
        let snap = self.load().or_status(500)?;
        let folder = snap.repo(&kind).folder.trim().to_string();
        if !tool.present_in(&folder) {
            return Err(ApiError::bad_request(format!(
                "{} is not in this repository",
                tool.dir()
            )));
        }
        self.store.save_repo_index(&snap, &kind, tool.id()).or_status(400)?;
        self.state_reply()
    }

    /// Chooses `tool` for a repository and starts its setup unless the folder already has it.
    fn begin_index(&self, req: &Request, tool: IndexTool) -> ApiResult<Response> {
        let kind = index_repo(req)?;
        let snap = self.load().or_status(500)?;
        self.store.save_repo_index(&snap, &kind, tool.id()).or_status(400)?;
        let folder = snap.repo(&kind).folder.trim().to_string();
        if tool.present_in(&folder) {
            return self.state_reply();
        }
        let job = match tool {
            IndexTool::Obsidian => self.jobs.start_obsidian(&kind, &folder),
            IndexTool::Codegraph => self.jobs.start_codegraph(&self.root, &kind, &folder),
        };
        Ok(write_json(202, &job.or_status(400)?))
    }

    /// Opens the Jira page of a ticket that belongs to a group.
    fn open_jira_ticket(&self, req: &Request) -> ApiResult<Response> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Body {
            key: String,
        }
        let body: Body = read_json(req)?;
        let cfg = self.store.atlassian().or_status(500)?;
        let groups = self.store.groups().or_status(500)?;
        let key = body.key.trim().to_uppercase();
        if !groups.iter().any(|group| group.epic_key == key) {
            return Err(ApiError::bad_request("That ticket is not on a group."));
        }
        let page = crate::open::jira_browse_page(&cfg.jira.url, &key).or_status(400)?;
        (self.browser)(&page).or_status(400)?;
        Ok(write_json(200, &json!({ "ok": true })))
    }

    /// Saves an index for a repository whose folder already holds exactly one index tool and has no index saved.
    /// Both folders means the choice stays unset.
    pub(crate) fn remember_present_indexes(&self, mut snap: console::Snapshot) -> anyhow::Result<console::Snapshot> {
        let mut found: Option<(String, IndexTool)> = None;
        for repo in snap.repos.iter_mut().filter(|repo| repo.index.trim().is_empty()) {
            let Some(tool) = sole_index_tool(&repo.folder) else {
                continue;
            };
            repo.index = tool.id().to_string();
            found = Some((repo.kind.clone(), tool));
        }
        let Some((kind, tool)) = found else { return Ok(snap) };
        self.store.save_repo_index(&snap, &kind, tool.id())?;
        self.load()
    }
}

/// The repository a setup call is for: the route's `{kind}`, else the body's `repo`.
fn index_repo(req: &Request) -> ApiResult<String> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Body {
        repo: String,
    }
    let mut kind = req.path_value("kind").trim().to_string();
    if kind.is_empty() {
        let body: Body = read_optional_json(req)?;
        kind = body.repo.trim().to_string();
    }
    if !toolchain::repo_id(&kind) {
        return Err(ApiError::bad_request("choose a repository"));
    }
    Ok(kind)
}
