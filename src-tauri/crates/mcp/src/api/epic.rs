//! Creating and deleting the Jira epic that belongs to one KIKI group.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use kiki_util::{json, Ctx};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::epic_fields::fallback_epic_fields;
use super::login::Login;
use super::{status_failed, transport_failed, AtlassianApi};
use crate::atlassian::Atlassian;
use crate::http::HttpRequest;
use crate::urlenc::path_escape;
use crate::wire::{clip_bytes, decode, nd, one_line};

/// Epic is a Jira epic created for one Kiki group.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Epic {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "BoardID")]
    pub board_id: String,
    #[serde(rename = "BoardName")]
    pub board_name: String,
    #[serde(rename = "URL")]
    pub url: String,
}

static ISSUE_KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9]{1,9}-[0-9]+$").unwrap());

/// The longest epic summary Jira accepts, in bytes.
const MAX_SUMMARY: usize = 255;

impl AtlassianApi {
    /// DeleteIssue deletes one Jira issue. An issue Jira no longer has is already gone.
    pub fn delete_issue(&self, ctx: &Ctx, cfg: &Atlassian, key: &str) -> Result<()> {
        let key = key.trim().to_uppercase();
        if !ISSUE_KEY_PATTERN.is_match(&key) {
            return Err(anyhow!("that ticket is not a Jira issue"));
        }
        let login = Login::for_service(cfg, &cfg.jira, "Jira")?;
        let url = format!("{}/rest/api/3/issue/{}", login.origin, path_escape(&key));
        let sent = self
            .transport
            .send(ctx, &HttpRequest::delete(&url, &login.user, &login.token));
        let response = sent.map_err(|err| transport_failed("Jira ticket", &err))?;
        if response.status == 404 {
            return Ok(());
        }
        match status_failed("Jira ticket", &response, jira_message) {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// CreateEpic creates an Epic on the Jira board saved in Settings.
    /// The summary is the group name. The description is the group description.
    pub fn create_epic(&self, ctx: &Ctx, cfg: &Atlassian, summary: &str, description: &str) -> Result<Epic> {
        let login = Login::for_service(cfg, &cfg.jira, "Jira")?;
        let board_id = cfg.jira.board_id.trim().to_string();
        if board_id.is_empty() {
            return Err(anyhow!("choose a Jira board in Settings before creating an epic"));
        }
        let summary = clip_bytes(summary.trim(), MAX_SUMMARY);
        if summary.is_empty() {
            return Err(anyhow!("enter a task name"));
        }
        let description = description.trim().to_string();
        let (project_key, mut board_name) = self.board_project(ctx, &login, &board_id)?;
        if board_name.is_empty() {
            board_name = cfg.jira.board_name.trim().to_string();
        }
        let fields = match self.epic_fields(ctx, &login, &project_key, &summary, &description) {
            Ok(fields) => fields,
            Err(_) => fallback_epic_fields(&project_key, &summary, &description),
        };
        let raw = json::marshal(&json!({ "fields": fields }));
        let url = format!("{}/rest/api/3/issue", login.origin);
        let body = self.write_ok(ctx, &url, &login, "Jira epic", raw.as_bytes())?;
        let created: Created = decode(&body).map_err(|e| anyhow!("Jira epic: {}", one_line(&e)))?;
        let key = created.key.trim().to_uppercase();
        if key.is_empty() {
            return Err(anyhow!("Jira created an epic but did not return its key"));
        }
        Ok(Epic {
            url: format!("{}/browse/{}", login.origin, path_escape(&key)),
            key,
            id: created.id.trim().to_string(),
            board_id,
            board_name,
        })
    }

    /// Returns (project key, board name) of a Jira board.
    fn board_project(&self, ctx: &Ctx, login: &Login, board_id: &str) -> Result<(String, String)> {
        let url = format!("{}/rest/agile/1.0/board/{}", login.origin, path_escape(board_id));
        let body = self.read_ok(ctx, &url, login, "Jira board")?;
        let board: Board = decode(&body).map_err(|e| anyhow!("Jira board: {}", one_line(&e)))?;
        let board_name = board.name.trim().to_string();
        let project_key = board.location.project_key.trim().to_uppercase();
        if !project_key.is_empty() {
            return Ok((project_key, board_name));
        }
        let url = format!(
            "{}/rest/agile/1.0/board/{}/project",
            login.origin,
            path_escape(board_id)
        );
        let listed = self.read_ok(ctx, &url, login, "Jira board")?;
        let projects: Projects = decode(&listed).map_err(|e| anyhow!("Jira board: {}", one_line(&e)))?;
        for project in projects.values {
            let project_key = project.key.trim().to_uppercase();
            if !project_key.is_empty() {
                return Ok((project_key, board_name));
            }
        }
        Err(anyhow!("Jira board {board_id} has no project"))
    }
}

#[derive(Deserialize, Default)]
struct Created {
    #[serde(default, deserialize_with = "nd")]
    id: String,
    #[serde(default, deserialize_with = "nd")]
    key: String,
}

#[derive(Deserialize, Default)]
struct Location {
    #[serde(default, rename = "projectKey", deserialize_with = "nd")]
    project_key: String,
}

#[derive(Deserialize, Default)]
struct Board {
    #[serde(default, deserialize_with = "nd")]
    name: String,
    #[serde(default, deserialize_with = "nd")]
    location: Location,
}

#[derive(Deserialize, Default)]
struct Project {
    #[serde(default, deserialize_with = "nd")]
    key: String,
}

#[derive(Deserialize, Default)]
struct Projects {
    #[serde(default, deserialize_with = "nd")]
    values: Vec<Project>,
}

/// The readable part of a Jira error body: its messages joined, or the body itself.
pub(super) fn jira_message(body: &[u8]) -> String {
    #[derive(Deserialize, Default)]
    struct Payload {
        #[serde(default, rename = "errorMessages", deserialize_with = "nd")]
        error_messages: Vec<String>,
        #[serde(default, deserialize_with = "nd")]
        errors: BTreeMap<String, String>,
        #[serde(default, deserialize_with = "nd")]
        message: String,
    }
    let payload: Payload = match decode(body) {
        Ok(payload) => payload,
        Err(_) => return one_line(&String::from_utf8_lossy(body)),
    };
    let mut parts: Vec<String> = Vec::new();
    parts.extend(payload.error_messages);
    for message in payload.errors.values() {
        let message = message.trim();
        if !message.is_empty() {
            parts.push(message.to_string());
        }
    }
    if !payload.message.trim().is_empty() {
        parts.push(payload.message);
    }
    if parts.is_empty() {
        return one_line(&String::from_utf8_lossy(body));
    }
    one_line(&parts.join(" "))
}
