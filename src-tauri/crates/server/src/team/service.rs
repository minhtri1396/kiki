//! The team folder operations: browse, save, list, compare, diff, copy. They take plain values and return
//! plain values (or an [`ApiError`] that carries the status), so they run without a request in tests.

use anyhow::{anyhow, Result};
use kiki_store::TeamFolder;
use kiki_teamfolder as teamfolder;
use kiki_toolchain as toolchain;
use kiki_util::path;
use serde_json::{json, Value};

use super::diff::{agent_file_text, diff_payload, repo_share_text};
use super::files::mkdir_all;
use super::import::RepoDecisions;
use super::shared_repo::publish_repos;
use super::views::{agent_views, compare_folder, empty_comparison, repo_views};
use super::{Direction, Outcome, Part, SETUP_FIRST};
use crate::api_error::{ApiError, ApiResult, OrStatus};
use crate::Server;

/// What `PUT /api/team` asks for.
pub(crate) struct SaveRequest {
    pub path: String,
    pub user: String,
    pub direction: String,
    pub use_existing: bool,
    pub repos: RepoDecisions,
}

/// What `POST /api/team/member` asks for.
pub(crate) struct MemberRequest {
    pub name: String,
    pub direction: String,
    pub part: String,
    pub repos: RepoDecisions,
}

fn conflict(payload: Value) -> ApiResult<Outcome> {
    Ok(Outcome::Conflict(payload))
}

impl Server {
    /// Opens the folder dialog and returns the chosen team folder.
    pub(crate) fn team_browse(&self) -> ApiResult<Value> {
        let _guard = self.begin_team()?;
        let picked = self.folder_picker.pick().or_status(400)?;
        let dir = teamfolder::dir(&picked).or_status(400)?;
        Ok(json!({ "path": dir }))
    }

    /// Saves the team folder and this user's name, copying the agents in the chosen direction. An empty `path`
    /// clears the setting.
    pub(crate) fn team_save(&self, req: SaveRequest) -> ApiResult<Outcome> {
        let team = req.path.trim();
        if !team.is_empty() && !teamfolder::valid_name(&req.user) {
            return Err(ApiError::new(400, "Enter your name."));
        }
        let _guard = self.begin_team()?;
        if team.is_empty() {
            self.store.save_team_folder(&TeamFolder::default()).or_status(500)?;
            return Ok(Outcome::Done);
        }
        let user_dir = teamfolder::user_dir(team, &req.user).or_status(400)?;
        let taken = self.name_taken(&user_dir).or_status(500)?;
        if taken && !req.use_existing {
            return conflict(json!({
                "error": "That name is already in the team folder.",
                "conflict": "name",
                "user": path::base(&user_dir),
            }));
        }
        let local = self.agent_dir().or_status(400)?;
        let direction = req.direction.trim();
        let exists = teamfolder::has_agents(&user_dir);
        match (exists, direction) {
            (true, "") => {
                return conflict(json!({
                    "error": "This user already has an agents folder.",
                    "conflict": "agents",
                    "path": path::dir(&user_dir),
                    "user": path::base(&user_dir),
                }));
            }
            (false, "pull") => return Err(ApiError::new(400, "That user folder has no agents folder.")),
            (true, "pull") => {
                let source = teamfolder::agents_dir(&user_dir);
                if let Outcome::Conflict(payload) = self.import_agents(&source, &local, &req.repos).or_status(400)? {
                    return conflict(payload);
                }
            }
            (_, "" | "push") => {
                mkdir_all(&user_dir, 0o755).or_status(400)?;
                teamfolder::copy(&local, &teamfolder::agents_dir(&user_dir)).or_status(400)?;
            }
            _ => return Err(ApiError::new(400, "Choose how to copy the agents.")),
        }
        let user = path::base(&user_dir);
        let saved = TeamFolder {
            path: path::dir(&user_dir),
            user: user.clone(),
            member: user,
        };
        self.store.save_team_folder(&saved).or_status(500)?;
        Ok(Outcome::Done)
    }

    /// Reports that `user_dir` is already a member folder that belongs to someone else.
    fn name_taken(&self, user_dir: &str) -> Result<bool> {
        let saved = self.store.team_folder()?;
        let dir = path::dir(user_dir);
        let name = path::base(user_dir);
        if path::clean(&saved.path) == dir && saved.user.trim() == name {
            return Ok(false);
        }
        Ok(teamfolder::members(&dir)?.contains(&name))
    }

    /// The members of a team folder: `path` when the caller names one, otherwise the saved folder (and then
    /// also which members differ from this app).
    pub(crate) fn team_members(&self, path_query: &str) -> ApiResult<Value> {
        let _guard = self.begin_team()?;
        let mut team = path_query.trim().to_string();
        if team.is_empty() {
            team = self.store.team_folder().or_status(500)?.path;
        }
        let dir = teamfolder::dir(&team).or_status(400)?;
        let names = teamfolder::members(&dir).or_status(400)?;
        let differs = if path_query.trim().is_empty() {
            self.members_that_differ(&dir, &names).or_status(500)?
        } else {
            Vec::new()
        };
        Ok(json!({ "members": names, "differs": differs, "path": dir }))
    }

    /// The saved team folder, which must have both a folder and a user name.
    fn configured_team(&self) -> ApiResult<TeamFolder> {
        let cfg = self.store.team_folder().or_status(500)?;
        if cfg.path.trim().is_empty() || cfg.user.trim().is_empty() {
            return Err(ApiError::new(400, SETUP_FIRST));
        }
        Ok(cfg)
    }

    /// This app, the user's folder and (when named) one member's folder, side by side.
    pub(crate) fn team_compare(&self, member_name: &str) -> ApiResult<Value> {
        let _guard = self.begin_team()?;
        let cfg = self.configured_team()?;
        let local = self.agent_dir().or_status(500)?;
        let app = agent_views(&local).or_status(500)?;
        let repo_dir = self.repo_dir().or_status(500)?;
        let app_repos = repo_views(&repo_dir).or_status(500)?;
        let user_dir = teamfolder::user_dir(&cfg.path, &cfg.user).or_status(400)?;
        let user = compare_folder(&local, &repo_dir, &user_dir, &cfg.user).or_status(500)?;
        let names = teamfolder::members(&cfg.path).or_status(400)?;
        let member_name = member_name.trim();
        let member = if member_name.is_empty() {
            empty_comparison("", false)
        } else {
            match teamfolder::member(&cfg.path, member_name) {
                Err(_) => empty_comparison(member_name, true),
                Ok(member_dir) => compare_folder(&local, &repo_dir, &member_dir, member_name).or_status(500)?,
            }
        };
        Ok(json!({
            "members": names,
            "app": app,
            "repos": app_repos,
            "user": user,
            "member": member,
        }))
    }

    /// The text diff of one agent or repository between this app and a member's folder.
    pub(crate) fn team_diff(&self, folder: &str, id: &str, part: &str) -> ApiResult<Value> {
        let _guard = self.begin_team()?;
        let (folder, id, part) = (folder.trim(), id.trim(), part.trim());
        if folder.is_empty() || id.is_empty() {
            return Err(ApiError::new(400, "Choose a file to compare."));
        }
        let Some(part) = Part::parse(part, Part::Agents) else {
            return Err(ApiError::new(400, "Choose agents or repositories to compare."));
        };
        let cfg = self.configured_team()?;
        let member_dir = teamfolder::member(&cfg.path, folder).or_status(400)?;
        match part {
            Part::Repos => self.repo_diff(&member_dir, folder, id),
            Part::Agents => self.agent_diff(&member_dir, folder, id),
        }
    }

    fn agent_diff(&self, member_dir: &str, folder: &str, id: &str) -> ApiResult<Value> {
        let local = self.agent_dir().or_status(500)?;
        let app = agent_file_text(&local, id).or_status(500)?;
        let theirs = agent_file_text(&teamfolder::agents_dir(member_dir), id).or_status(500)?;
        if app.is_none() && theirs.is_none() {
            return Err(ApiError::new(404, "That agent is not in this app or that folder."));
        }
        let (app_text, app_name) = app.map(|a| (a.text, a.name)).unwrap_or_default();
        let (folder_text, folder_name) = theirs.map(|a| (a.text, a.name)).unwrap_or_default();
        let name = [app_name, folder_name, id.to_string()]
            .into_iter()
            .find(|n| !n.is_empty())
            .unwrap_or_default();
        Ok(diff_payload(id, &name, folder, &app_text, &folder_text))
    }

    fn repo_diff(&self, member_dir: &str, folder: &str, id: &str) -> ApiResult<Value> {
        if !toolchain::repo_id(id) {
            return Err(ApiError::new(400, "Choose a repository to compare."));
        }
        let local = self.repo_dir().or_status(500)?;
        let app = repo_share_text(&local, id).or_status(500)?;
        let theirs = repo_share_text(&teamfolder::repos_dir(member_dir), id).or_status(500)?;
        if app.is_none() && theirs.is_none() {
            return Err(ApiError::new(404, "That repository is not in this app or that folder."));
        }
        let app_text = app.unwrap_or_default();
        let folder_text = theirs.unwrap_or_default();
        Ok(diff_payload(id, id, folder, &app_text, &folder_text))
    }

    /// Copies a member's agents or repositories into this app (`pull`) or this app's into the member's folder
    /// (`push`), then selects that member.
    pub(crate) fn team_member(&self, req: MemberRequest) -> ApiResult<Outcome> {
        let _guard = self.begin_team()?;
        let mut cfg = self.configured_team()?;
        let member = teamfolder::member(&cfg.path, &req.name).or_status(400)?;
        let direction = match req.direction.trim() {
            "" => "pull",
            other => other,
        };
        let Some(part) = Part::parse(req.part.trim(), Part::Agents) else {
            return Err(ApiError::new(400, "Choose agents or repositories to copy."));
        };
        match part {
            Part::Repos => self.copy_repos(&member, direction).or_status(400)?,
            Part::Agents => {
                let local = self.agent_dir().or_status(400)?;
                match Direction::parse(direction) {
                    Some(Direction::Pull) => {
                        if !teamfolder::has_agents(&member) {
                            return Err(ApiError::new(400, "That member folder has no agents folder."));
                        }
                        let source = teamfolder::agents_dir(&member);
                        if let Outcome::Conflict(payload) =
                            self.import_agents(&source, &local, &req.repos).or_status(400)?
                        {
                            return conflict(payload);
                        }
                    }
                    Some(Direction::Push) => {
                        teamfolder::copy(&local, &teamfolder::agents_dir(&member)).or_status(400)?;
                    }
                    None => return Err(ApiError::new(400, "Choose how to copy the agents.")),
                }
            }
        }
        cfg.member = path::base(&member);
        self.store.save_team_folder(&cfg).or_status(500)?;
        Ok(Outcome::Done)
    }

    /// Writes `.kiki/agents` to this user's folder in the team folder.
    pub(crate) fn save_user_agents(&self) -> ApiResult<()> {
        let _guard = self.begin_team()?;
        let cfg = self.configured_team()?;
        let local = self.agent_dir().or_status(400)?;
        let user_dir = teamfolder::user_dir(&cfg.path, &cfg.user).or_status(400)?;
        mkdir_all(&user_dir, 0o755).or_status(400)?;
        teamfolder::copy(&local, &teamfolder::agents_dir(&user_dir)).or_status(400)?;
        Ok(())
    }

    /// Writes this app's shared repository files to this user's folder in the team folder.
    pub(crate) fn save_user_repos(&self) -> ApiResult<()> {
        let _guard = self.begin_team()?;
        let cfg = self.configured_team()?;
        let user_dir = teamfolder::user_dir(&cfg.path, &cfg.user).or_status(400)?;
        mkdir_all(&user_dir, 0o755).or_status(400)?;
        let local = self.repo_dir().or_status(400)?;
        publish_repos(&local, &teamfolder::repos_dir(&user_dir)).or_status(400)?;
        Ok(())
    }

    fn copy_repos(&self, member_dir: &str, direction: &str) -> Result<()> {
        let local = self.repo_dir()?;
        match Direction::parse(direction) {
            Some(Direction::Pull) => self.adopt_repos(&teamfolder::repos_dir(member_dir)),
            Some(Direction::Push) => publish_repos(&local, &teamfolder::repos_dir(member_dir)),
            None => Err(anyhow!("Choose how to copy the repositories.")),
        }
    }

    /// The folder where this app keeps its shared repository files (`repos` next to the setup file).
    pub(crate) fn repo_dir(&self) -> Result<String> {
        let file = self.store.file().trim();
        if file.is_empty() || !path::is_abs(file) {
            return Err(anyhow!("The repository folder is not available."));
        }
        Ok(path::join(&[&path::dir(file), "repos"]))
    }
}
