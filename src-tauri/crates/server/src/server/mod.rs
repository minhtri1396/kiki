//! Pattern: Facade. [`Server`] owns the shared state and services (store, jobs, Orca, model cache, guardrail
//! state) and routes every request; each area of the API lives in its own module with a controller.

mod agent_form;
mod agent_json;
mod agents;
mod builder;
mod catalog;
#[cfg(test)]
mod catalog_tests;
mod generate;
mod index_tool;
mod indexes;
mod job_routes;
mod models;
mod orchestrator;
mod prepare;
mod settings;
mod setup_form;
mod state;
mod state_rows;
mod support;
mod worktree;

use std::sync::{Arc, Mutex};

use anyhow::Result;

use kiki_agentfile as agentfile;
use kiki_guard::Binding;
use kiki_jobs::Manager;
use kiki_mcp as mcp;
use kiki_orca::Client;
use kiki_store::Store;
use kiki_util::Ctx;

use crate::http::{HttpServer, Request, Response, Router};
use crate::team::FolderPicker;

pub use builder::ServerBuilder;
pub use catalog::NamedId;
pub(crate) use catalog::{tools_from, tools_ready, MCP_ATLASSIAN, MCP_CONFLUENCE, MCP_JIRA};
pub(crate) use models::ModelsState;
pub use prepare::LaunchPrep;

/// The area controllers, as one list for the router.
pub(crate) mod controllers {
    pub(crate) use super::agents::AgentController;
    pub(crate) use super::generate::GenerateController;
    pub(crate) use super::indexes::IndexController;
    pub(crate) use super::job_routes::JobController;
    pub(crate) use super::models::ModelController;
    pub(crate) use super::settings::SettingsController;
    pub(crate) use super::state::StateController;
}

/// Installs or removes a command hook for the user at `home`: `(home, on)`.
pub type RtkHooksFn = Arc<dyn Fn(&str, bool) -> Result<()> + Send + Sync>;
/// Installs or removes the guardrail hooks of every AI tool: `(home, on)`.
pub type GuardHooksFn = Arc<dyn Fn(&str, bool) -> Result<()> + Send + Sync>;
/// Rewrites a task name into a short English name for a git branch: `(ctx, tool, model, name)`.
pub type BranchNamerFn = Arc<dyn Fn(&Ctx, kiki_aitool::AiTool, &str, &str) -> Result<String> + Send + Sync>;
/// Opens a page in the system browser.
pub type BrowserFn = Arc<dyn Fn(&str) -> Result<()> + Send + Sync>;

/// What guardrails know: the home they are installed for, the address of the running app, and the folders
/// each running agent may touch.
#[derive(Default)]
pub struct GuardState {
    pub home: String,
    pub url: String,
    pub bindings: Vec<Binding>,
}

/// Serves the console. Share it as `Arc<Server>`; every method takes `&self`.
pub struct Server {
    pub root: String,
    pub store: Arc<Store>,
    pub orca: Client,
    pub jobs: Arc<Manager>,
    /// Jira and Confluence calls. The default talks to the real network.
    pub atlassian: mcp::AtlassianApi,
    pub models: Mutex<ModelsState>,
    /// Serializes the team folder operations.
    pub team_mu: Mutex<()>,
    pub guard: Arc<Mutex<GuardState>>,
    /// The home directory used when installing Atlassian MCP and the command hooks; empty uses the current user.
    pub mcp_home: String,
    /// `None` uses `kiki_tools::sync_hooks`.
    pub rtk_hooks: Option<RtkHooksFn>,
    /// `None` uses the guard crate.
    pub guard_hooks: Option<GuardHooksFn>,
    /// `None` uses the Generate text AI tool.
    pub branch_namer: Option<BranchNamerFn>,
    /// Opens a page in the system browser (the default asks the operating system).
    pub browser: BrowserFn,
    /// Asks the user to choose a folder for the team folder and repositories (the default is the system dialog).
    pub folder_picker: FolderPicker,
    router: Router<Server>,
}

impl Server {
    /// A server with every service at its production default.
    pub fn new(root: impl Into<String>, store: Arc<Store>, orca: Client, jobs: Arc<Manager>) -> Server {
        Server::builder(root, store, orca, jobs).build()
    }

    /// Starts a [`ServerBuilder`].
    pub fn builder(root: impl Into<String>, store: Arc<Store>, orca: Client, jobs: Arc<Manager>) -> ServerBuilder {
        ServerBuilder::new(root, store, orca, jobs)
    }

    /// Binds `addr` and serves this console on a background thread.
    /// The returned handle can `stopper()` / `shutdown()` the listener.
    pub fn serve(self: Arc<Server>, addr: &str) -> Result<Arc<HttpServer>> {
        let listener = Arc::new(HttpServer::bind(addr)?);
        let running = listener.clone();
        std::thread::spawn(move || running.serve(move |req| self.handle(&req)));
        Ok(listener)
    }

    /// Answers one request.
    pub fn handle(&self, req: &Request) -> Response {
        self.router.dispatch(self, req)
    }

    /// The saved setup with the console root applied.
    pub(crate) fn load(&self) -> Result<kiki_console::Snapshot> {
        let snap = self.store.load()?;
        let mut snap = kiki_console::apply_root(&self.root, snap)?;
        snap.setup_file = self.store.file().to_string();
        Ok(snap)
    }

    /// The folder that holds the agent files.
    pub(crate) fn agent_dir(&self) -> Result<String> {
        agentfile::resolve(&self.root, "")
    }

    /// Creates the agent folder and any missing agent file.
    pub(crate) fn sync_agent_files(&self) -> Result<()> {
        ensure_agent_files(&self.root)?;
        Ok(())
    }
}

/// Creates the agent folder and any missing agent markdown files.
/// The first time that folder is empty, each new file starts on cursor. After that, each agent keeps its own AI tool.
pub fn ensure_agent_files(root: &str) -> Result<String> {
    let dir = agentfile::resolve(root, "")?;
    agentfile::ensure(&dir, "cursor")?;
    Ok(dir)
}
