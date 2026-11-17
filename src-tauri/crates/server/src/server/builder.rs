//! Pattern: Builder. `Server::builder(..)` sets the optional services (home folder, hooks, branch namer, Atlassian
//! transport, browser) with named calls instead of public fields assigned after construction.

use std::sync::{Arc, Mutex};

use anyhow::Result;

use kiki_aitool::AiTool;
use kiki_jobs::Manager;
use kiki_mcp::AtlassianApi;
use kiki_orca::Client;
use kiki_store::Store;
use kiki_util::Ctx;

use super::{BranchNamerFn, BrowserFn, GuardHooksFn, GuardState, ModelsState, RtkHooksFn, Server};
use crate::controller::build_router;
use crate::team::FolderPicker;

/// Collects the services of a [`Server`]. Everything but the four required parts has a production default.
pub struct ServerBuilder {
    root: String,
    store: Arc<Store>,
    orca: Client,
    jobs: Arc<Manager>,
    atlassian: AtlassianApi,
    mcp_home: String,
    rtk_hooks: Option<RtkHooksFn>,
    guard_hooks: Option<GuardHooksFn>,
    branch_namer: Option<BranchNamerFn>,
    browser: BrowserFn,
    folder_picker: FolderPicker,
}

impl ServerBuilder {
    pub(super) fn new(root: impl Into<String>, store: Arc<Store>, orca: Client, jobs: Arc<Manager>) -> ServerBuilder {
        ServerBuilder {
            root: root.into(),
            store,
            orca,
            jobs,
            atlassian: AtlassianApi::default(),
            mcp_home: String::new(),
            rtk_hooks: None,
            guard_hooks: None,
            branch_namer: None,
            browser: Arc::new(crate::open::open_in_browser),
            folder_picker: FolderPicker::default(),
        }
    }

    /// The home directory used when installing Atlassian MCP and the command hooks.
    pub fn mcp_home(mut self, home: impl Into<String>) -> ServerBuilder {
        self.mcp_home = home.into();
        self
    }

    /// Replaces the installer of the RTK command hook.
    pub fn rtk_hooks(mut self, hooks: impl Fn(&str, bool) -> Result<()> + Send + Sync + 'static) -> ServerBuilder {
        self.rtk_hooks = Some(Arc::new(hooks));
        self
    }

    /// Replaces the installer of the guardrail hooks.
    pub fn guard_hooks(mut self, hooks: impl Fn(&str, bool) -> Result<()> + Send + Sync + 'static) -> ServerBuilder {
        self.guard_hooks = Some(Arc::new(hooks));
        self
    }

    /// Replaces the AI call that names a task's git branch.
    pub fn branch_namer(
        mut self,
        namer: impl Fn(&Ctx, AiTool, &str, &str) -> Result<String> + Send + Sync + 'static,
    ) -> ServerBuilder {
        self.branch_namer = Some(Arc::new(namer));
        self
    }

    /// Replaces the Jira and Confluence client (for example to give it a fake transport).
    pub fn atlassian(mut self, api: AtlassianApi) -> ServerBuilder {
        self.atlassian = api;
        self
    }

    /// Replaces the function that opens a page in the system browser.
    pub fn browser(mut self, open: impl Fn(&str) -> Result<()> + Send + Sync + 'static) -> ServerBuilder {
        self.browser = Arc::new(open);
        self
    }

    /// Replaces the folder dialog (for example with a closure that returns a fixed folder).
    pub fn folder_picker(mut self, picker: FolderPicker) -> ServerBuilder {
        self.folder_picker = picker;
        self
    }

    pub fn build(self) -> Server {
        Server {
            root: self.root,
            store: self.store,
            orca: self.orca,
            jobs: self.jobs,
            atlassian: self.atlassian,
            models: Mutex::new(ModelsState::default()),
            team_mu: Mutex::new(()),
            guard: Arc::new(Mutex::new(GuardState::default())),
            mcp_home: self.mcp_home,
            rtk_hooks: self.rtk_hooks,
            guard_hooks: self.guard_hooks,
            branch_namer: self.branch_namer,
            browser: self.browser,
            folder_picker: self.folder_picker,
            router: build_router(),
        }
    }
}
