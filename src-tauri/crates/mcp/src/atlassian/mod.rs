//! The saved Atlassian setup (Jira and Confluence): the model, its validation, and the environment
//! `mcp-atlassian` reads.

mod compat;
mod site;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use compat::lenient_string;
use site::validate_site;
pub(crate) use site::{clean_url, parse_site};

fn is_false(value: &bool) -> bool {
    !*value
}

// serde's `skip_serializing_if` hands over `&String`.
#[allow(clippy::ptr_arg)]
fn is_blank(value: &String) -> bool {
    value.is_empty()
}

/// Service is Jira or Confluence inside the Atlassian MCP setup.
/// Own means this service uses its own email and API token.
/// Otherwise it uses the email and API token saved on Atlassian.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    #[serde(default, skip_serializing_if = "is_false")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub url: String,
    #[serde(default, rename = "own_credentials", skip_serializing_if = "is_false")]
    pub own: bool,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub username: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub api_token: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub board_id: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub board_name: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub space_id: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub space_key: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub space_name: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub parent_id: String,
    #[serde(default, skip_serializing_if = "is_blank", deserialize_with = "lenient_string")]
    pub parent_title: String,
}

/// Atlassian is the MCP setup saved for Jira and Confluence.
/// The email and API token on Atlassian are shared. A service can override them.
///
/// Serialization omits every empty field except `enabled`, and leaves out a service
/// whose fields are all empty (the layout of `mcp.yaml` files already on disk).
/// Deserialization also reads older files that stored credentials on each product.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Atlassian {
    pub enabled: bool,
    #[serde(skip_serializing_if = "is_blank")]
    pub username: String,
    #[serde(skip_serializing_if = "is_blank")]
    pub api_token: String,
    #[serde(skip_serializing_if = "Service::is_zero")]
    pub jira: Service,
    #[serde(skip_serializing_if = "Service::is_zero")]
    pub confluence: Service,
}

impl Atlassian {
    /// Empty reports that nothing is worth storing.
    pub fn empty(&self) -> bool {
        let a = self.normalized();
        !a.enabled && a.username.is_empty() && a.api_token.is_empty() && a.jira.cleared() && a.confluence.cleared()
    }

    /// Ready reports that a run should install Atlassian MCP.
    pub fn ready(&self) -> bool {
        let a = self.normalized();
        a.enabled && (a.jira.enabled || a.confluence.enabled)
    }

    /// Normalized trims fields and drops a trailing slash from each site URL.
    pub fn normalized(&self) -> Atlassian {
        let mut a = self.clone();
        a.username = a.username.trim().to_string();
        a.api_token = a.api_token.trim().to_string();
        a.jira = a.jira.clean();
        a.confluence = a.confluence.clean();
        a
    }

    /// Validate checks a setup that will be stored or written into an AI tool.
    /// A setup that is off, or completely empty, is valid.
    pub fn validate(&self) -> Result<()> {
        let a = self.normalized();
        if !a.enabled {
            return Ok(());
        }
        if !a.jira.enabled && !a.confluence.enabled {
            return Err(anyhow!("turn on Jira or Confluence"));
        }
        let shared_needed = (a.jira.enabled && !a.jira.own) || (a.confluence.enabled && !a.confluence.own);
        if shared_needed && !has_credential(&a.username, &a.api_token) {
            return Err(anyhow!("Atlassian needs an email and an API token"));
        }
        a.jira.validate("Jira")?;
        a.confluence.validate("Confluence")
    }

    /// Env is the environment mcp-atlassian reads. Empty fields are left out.
    /// A service that is off is left out. A service without its own email and API token
    /// uses the shared ones.
    pub fn env(&self) -> BTreeMap<String, String> {
        let a = self.normalized();
        if !a.ready() {
            return BTreeMap::new();
        }
        let mut out = BTreeMap::new();
        if a.jira.enabled {
            for (key, value) in a.service_env("JIRA", &a.jira) {
                out.insert(key, value);
            }
        }
        if a.confluence.enabled {
            for (key, value) in a.service_env("CONFLUENCE", &a.confluence) {
                out.insert(key, value);
            }
        }
        out
    }

    fn service_env(&self, prefix: &str, service: &Service) -> BTreeMap<String, String> {
        let (mut user, mut api) = (self.username.clone(), self.api_token.clone());
        if service.own {
            user = service.username.clone();
            api = service.api_token.clone();
        }
        let mut out = BTreeMap::new();
        if !service.url.is_empty() {
            out.insert(format!("{prefix}_URL"), service.url.clone());
        }
        if !user.is_empty() {
            out.insert(format!("{prefix}_USERNAME"), user);
        }
        if !api.is_empty() {
            out.insert(format!("{prefix}_API_TOKEN"), api);
        }
        out
    }
}

/// Apply merges a settings form into the saved setup.
/// A blank token keeps the saved token. A form left entirely blank clears the setup.
pub fn apply(prev: &Atlassian, form: &Atlassian) -> Atlassian {
    let prev = prev.normalized();
    let form = form.normalized();
    if form.empty() {
        return Atlassian::default();
    }
    let mut next = form.clone();
    if next.api_token.is_empty() {
        next.api_token = prev.api_token.clone();
    }
    next.jira = apply_service(&prev.jira, &form.jira);
    next.confluence = apply_service(&prev.confluence, &form.confluence);
    next
}

fn apply_service(prev: &Service, form: &Service) -> Service {
    let mut next = form.clone();
    if next.api_token.is_empty() {
        next.api_token = prev.api_token.clone();
    }
    next
}

impl Service {
    /// A service with every field empty; it is left out of the saved file.
    fn is_zero(&self) -> bool {
        *self == Service::default()
    }

    pub(crate) fn clean(&self) -> Service {
        let mut s = self.clone();
        s.url = clean_url(&s.url);
        s.username = s.username.trim().to_string();
        s.api_token = s.api_token.trim().to_string();
        s.board_id = s.board_id.trim().to_string();
        s.board_name = s.board_name.trim().to_string();
        s.space_id = s.space_id.trim().to_string();
        s.space_key = s.space_key.trim().to_string();
        s.space_name = s.space_name.trim().to_string();
        s.parent_id = s.parent_id.trim().to_string();
        s.parent_title = s.parent_title.trim().to_string();
        s
    }

    fn cleared(&self) -> bool {
        !self.enabled
            && !self.own
            && self.url.is_empty()
            && self.username.is_empty()
            && self.api_token.is_empty()
            && self.board_id.is_empty()
            && self.board_name.is_empty()
            && self.space_id.is_empty()
            && self.space_key.is_empty()
            && self.space_name.is_empty()
            && self.parent_id.is_empty()
            && self.parent_title.is_empty()
    }

    fn validate(&self, name: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.url.is_empty() {
            return Err(anyhow!("{name} site is required"));
        }
        if let Err(err) = validate_site(&self.url) {
            return Err(anyhow!("{name} site {err}"));
        }
        if self.own && !has_credential(&self.username, &self.api_token) {
            return Err(anyhow!("{name} needs an email and an API token"));
        }
        Ok(())
    }

    fn present(&self) -> bool {
        !self.url.is_empty() || self.has_credential()
    }

    fn has_credential(&self) -> bool {
        has_credential(&self.username, &self.api_token)
    }
}

pub(crate) fn has_credential(username: &str, api: &str) -> bool {
    !username.is_empty() && !api.is_empty()
}
