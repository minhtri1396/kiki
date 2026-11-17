//! Pattern: Facade. `AtlassianApi` is the one entry point for everything KIKI asks Jira and Confluence
//! (`boards`, `spaces`, `pages`, `create_epic`, `delete_issue`); it hides logins, URLs, status handling, and
//! JSON shapes, and sends through an injected [`HttpTransport`].

mod catalog;
mod epic;
mod epic_fields;
mod login;
#[cfg(test)]
mod tests;

pub use catalog::Hit;
pub use epic::Epic;

use std::sync::Arc;

use anyhow::{anyhow, Error, Result};
use kiki_util::Ctx;

use crate::http::{status_text, HttpRequest, HttpResponse, HttpTransport, UreqTransport};
use crate::wire::one_line;
use login::Login;

/// Jira and Confluence calls over an injected HTTP transport. `Default` talks to the real network.
#[derive(Clone)]
pub struct AtlassianApi {
    transport: Arc<dyn HttpTransport>,
}

impl Default for AtlassianApi {
    fn default() -> Self {
        AtlassianApi::new(Arc::new(UreqTransport))
    }
}

impl AtlassianApi {
    pub fn new(transport: Arc<dyn HttpTransport>) -> AtlassianApi {
        AtlassianApi { transport }
    }

    /// GET with a 2xx check; returns the body.
    fn read_ok(&self, ctx: &Ctx, url: &str, login: &Login, label: &str) -> Result<Vec<u8>> {
        let sent = self
            .transport
            .send(ctx, &HttpRequest::get(url, &login.user, &login.token));
        settle(label, sent, |body| one_line(&String::from_utf8_lossy(body)))
    }

    /// POST a JSON payload with a 2xx check; returns the body. Failures carry Jira's own message.
    fn write_ok(&self, ctx: &Ctx, url: &str, login: &Login, label: &str, payload: &[u8]) -> Result<Vec<u8>> {
        let sent = self
            .transport
            .send(ctx, &HttpRequest::post(url, &login.user, &login.token, payload));
        settle(label, sent, epic::jira_message)
    }
}

/// Error for a request that never got an answer.
fn transport_failed(label: &str, err: &Error) -> Error {
    anyhow!("{label}: {}", one_line(&format!("{err:#}")))
}

/// Error for a non-2xx answer: rejected credentials, or `describe` of the body (or the status text).
/// `None` means the answer was a success.
fn status_failed(label: &str, response: &HttpResponse, describe: fn(&[u8]) -> String) -> Option<Error> {
    let status = response.status;
    if status == 401 || status == 403 {
        return Some(anyhow!("{label}: Atlassian rejected the email or API token"));
    }
    if (200..300).contains(&status) {
        return None;
    }
    let mut message = describe(&response.body);
    if message.is_empty() {
        message = status_text(status).to_string();
    }
    Some(anyhow!("{label}: {message}"))
}

/// Turns one transport result into the body of a successful answer.
fn settle(label: &str, sent: Result<HttpResponse>, describe: fn(&[u8]) -> String) -> Result<Vec<u8>> {
    let response = sent.map_err(|err| transport_failed(label, &err))?;
    match status_failed(label, &response, describe) {
        Some(err) => Err(err),
        None => Ok(response.body),
    }
}
