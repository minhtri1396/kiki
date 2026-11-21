//! Settings: the setup form, repository folders, the tools and MCP servers turned on, and the RTK command hook.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;

use kiki_console as console;
use kiki_mcp as mcp;
use kiki_toolchain as toolchain;
use kiki_tools as tools;
use kiki_util::path;

use super::catalog::{tools_from, NamedId};
use super::setup_form::SetupForm;
use super::support::is_dir;
use super::Server;
use crate::api_error::{ApiError, ApiResult, OrStatus};
use crate::controller::Controller;
use crate::http::{read_json, write_json, Router};

/// Pattern: Controller. Settings and repository folder routes.
pub(crate) struct SettingsController;

impl Controller for SettingsController {
    fn register(router: &mut Router<Server>) {
        router.api("PUT /api/setup", |server, req| server.save_setup(read_json(req)?));
        router.api("POST /api/repos/services", |_, req| {
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct Body {
                folder: String,
            }
            let body: Body = read_json(req)?;
            let names = console::scan_services(&body.folder).or_status(400)?;
            Ok(write_json(200, &json!({ "services": names })))
        });
        router.api("POST /api/repos/browse", |server, _| {
            let picked = server.folder_picker.pick().or_status(400)?;
            let dir = path::clean(picked.trim());
            if !is_dir(&dir) {
                return Err(ApiError::bad_request(format!("folder {dir} does not exist")));
            }
            Ok(write_json(200, &json!({ "path": dir })))
        });
        router.api("POST /api/repos", |server, req| server.create_repo(read_json(req)?));
        router.api("PUT /api/repos/{kind}", |server, req| {
            server.rename_repo(req.path_value("kind"), read_json(req)?)
        });
        router.api("DELETE /api/repos/{kind}", |server, req| {
            server.delete_repo(req.path_value("kind"))
        });
        router.api("PUT /api/repos/{kind}/folder", |server, req| {
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct Body {
                folder: String,
            }
            let kind = known_repo(req.path_value("kind"))?;
            let body: Body = read_json(req)?;
            let snap = server.load().or_status(500)?;
            server
                .store
                .save_repo_folder(&snap, &kind, &body.folder)
                .or_status(400)?;
            server.state_reply()
        });
    }
}

/// The trimmed repository id from a route, or the 400 reply when it is not a repository id.
pub(super) fn known_repo(raw: &str) -> ApiResult<String> {
    let kind = raw.trim();
    if !toolchain::repo_id(kind) {
        return Err(ApiError::bad_request("unknown repository"));
    }
    Ok(kind.to_string())
}

impl Server {
    /// Saves the Settings form, then brings hooks and agent files in line with it.
    fn save_setup(&self, form: SetupForm) -> ApiResult<crate::http::Response> {
        let mut snap = form.snapshot(&self.root);
        let current = self.load().or_status(500)?;
        SetupForm::keep_saved(&mut snap, &current);
        let snap = console::apply_root(&self.root, snap).or_status(400)?;
        console::validate(&snap).or_status(400)?;
        let atlassian = self.merged_atlassian(&form)?;
        self.store.save(&snap).or_status(500)?;
        if let Some(next) = &atlassian {
            self.store.save_atlassian(next).or_status(400)?;
        }
        if let Some(settings) = &form.tools {
            self.save_tools(settings)?;
        }
        self.sync_agent_files().or_status(500)?;
        self.state_reply()
    }

    /// The saved Atlassian setup with the form's changes applied and checked; `None` when the form has none.
    fn merged_atlassian(&self, form: &SetupForm) -> ApiResult<Option<mcp::Atlassian>> {
        let Some(body) = &form.atlassian else { return Ok(None) };
        let saved = self.store.atlassian().or_status(500)?;
        let next = mcp::apply(&saved, &body.form());
        next.validate().or_status(400)?;
        Ok(Some(next))
    }

    /// Saves the tool choices and installs or removes the hooks they turn on.
    fn save_tools(&self, settings: &tools::Settings) -> ApiResult<()> {
        let mut next = settings.clone();
        if next.guardrails.is_none() {
            next.guardrails = self.load_tools().or_status(500)?.guardrails;
        }
        self.store.save_tools(&next).or_status(400)?;
        self.sync_rtk_hook(next.rtk).or_status(400)?;
        self.sync_guardrails(next.guardrails_on()).or_status(400)?;
        Ok(())
    }

    /// The Atlassian section saved in Settings.
    pub(crate) fn atlassian_setup(&self) -> Result<mcp::Atlassian> {
        self.store.atlassian()
    }

    /// The tool choices saved in Settings.
    pub(crate) fn load_tools(&self) -> Result<tools::Settings> {
        self.store.tools()
    }

    /// The tools turned on in Settings.
    pub(crate) fn enabled_tools(&self) -> Result<Vec<NamedId>> {
        Ok(tools_from(&self.load_tools()?))
    }

    /// Registers the RTK hook when Settings has RTK on, and removes it when RTK is off.
    pub fn sync_rtk(&self) -> Result<()> {
        let cfg = self.load_tools()?;
        self.sync_rtk_hook(cfg.rtk)
    }

    /// Installs (`on`) or removes the RTK hook for the configured home.
    pub(crate) fn sync_rtk_hook(&self, on: bool) -> Result<()> {
        let home = self.hook_home()?;
        match &self.rtk_hooks {
            Some(hooks) => hooks(&home, on),
            None => tools::sync_hooks(&home, on),
        }
    }

    /// The home folder hooks and MCP settings are written under: the configured one, or the user's `$HOME`.
    pub(crate) fn hook_home(&self) -> Result<String> {
        let home = self.mcp_home.trim();
        if !home.is_empty() {
            return Ok(home.to_string());
        }
        // A test that reaches this point would install hooks into the developer's real home folder.
        #[cfg(test)]
        return Err(anyhow!("tests must set `mcp_home` to a temporary folder"));
        #[cfg(not(test))]
        match std::env::var("HOME") {
            Ok(home) if !home.is_empty() => Ok(home),
            _ => Err(anyhow!("$HOME is not defined")),
        }
    }
}
