//! The `teamFolder` object of the state payload, and the single-operation lock around team folder work.

use std::sync::{MutexGuard, TryLockError};

use anyhow::Result;
use kiki_store::TeamFolder;
use kiki_teamfolder as teamfolder;
use kiki_util::path;
use serde_json::{json, Value};

use crate::api_error::{ApiError, ApiResult};
use crate::Server;

impl Server {
    /// Only one team folder operation runs at a time; a second one is answered with 409.
    pub(super) fn begin_team(&self) -> ApiResult<MutexGuard<'_, ()>> {
        match self.team_mu.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => Err(ApiError::new(409, "Team folder setup is already running.")),
        }
    }

    /// The `teamFolder` object of the state payload.
    pub(crate) fn team_status(&self) -> Result<Value> {
        let cfg = self.store.team_folder()?;
        let stale = self.member_stale(&cfg)?;
        let user_stale = self.user_stale(&cfg)?;
        Ok(json!({
            "path": cfg.path,
            "user": cfg.user,
            "member": cfg.member,
            "stale": stale,
            "userStale": user_stale,
        }))
    }

    /// Reports that the selected member's agents differ from `.kiki/agents`.
    fn member_stale(&self, cfg: &TeamFolder) -> Result<bool> {
        let (team, user, name) = (cfg.path.trim(), cfg.user.trim(), cfg.member.trim());
        if team.is_empty() || user.is_empty() || name.is_empty() {
            return Ok(false);
        }
        let member = match teamfolder::member(team, name) {
            Ok(m) if teamfolder::has_agents(&m) => m,
            _ => return Ok(true),
        };
        let local = self.agent_dir()?;
        Ok(!teamfolder::same(&teamfolder::agents_dir(&member), &local)?)
    }

    /// Reports that `.kiki/agents` differs from this user's agents folder.
    fn user_stale(&self, cfg: &TeamFolder) -> Result<bool> {
        let (team, user) = (cfg.path.trim(), cfg.user.trim());
        if team.is_empty() || user.is_empty() {
            return Ok(false);
        }
        let user_dir = teamfolder::user_dir(team, user)?;
        let local = self.agent_dir()?;
        if !teamfolder::has_agents(&user_dir) {
            return Ok(true);
        }
        Ok(!teamfolder::same(&local, &teamfolder::agents_dir(&user_dir))?)
    }

    /// The members whose agents folder is missing or not the same as `.kiki/agents`.
    pub(super) fn members_that_differ(&self, dir: &str, names: &[String]) -> Result<Vec<String>> {
        let local = self.agent_dir()?;
        let mut differs = Vec::new();
        for name in names {
            let member = path::join(&[dir, name]);
            if !teamfolder::has_agents(&member) || !teamfolder::same(&teamfolder::agents_dir(&member), &local)? {
                differs.push(name.clone());
            }
        }
        Ok(differs)
    }
}
