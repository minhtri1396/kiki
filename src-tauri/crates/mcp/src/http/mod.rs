//! Pattern: Adapter + Strategy. `HttpTransport` is the one seam between the Atlassian client and the network;
//! `UreqTransport` adapts `ureq` to it, and tests inject a fake instead of replacing process globals.

mod status;
mod ureq_transport;

pub use status::status_text;
pub use ureq_transport::UreqTransport;

use anyhow::Result;
use kiki_util::Ctx;

/// The HTTP verbs the Atlassian client uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

impl Method {
    /// The verb on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Delete => "DELETE",
        }
    }

    /// The verb as it appears in transport error messages (`Get "https://..."`).
    pub(crate) fn verb(self) -> &'static str {
        match self {
            Method::Get => "Get",
            Method::Post => "Post",
            Method::Delete => "Delete",
        }
    }
}

/// One basic-auth JSON request.
#[derive(Debug, Clone, Copy)]
pub struct HttpRequest<'a> {
    pub method: Method,
    pub url: &'a str,
    pub user: &'a str,
    pub token: &'a str,
    /// JSON body, sent with `Content-Type: application/json`.
    pub payload: Option<&'a [u8]>,
}

impl<'a> HttpRequest<'a> {
    pub fn get(url: &'a str, user: &'a str, token: &'a str) -> Self {
        HttpRequest {
            method: Method::Get,
            url,
            user,
            token,
            payload: None,
        }
    }

    pub fn post(url: &'a str, user: &'a str, token: &'a str, payload: &'a [u8]) -> Self {
        HttpRequest {
            method: Method::Post,
            url,
            user,
            token,
            payload: Some(payload),
        }
    }

    pub fn delete(url: &'a str, user: &'a str, token: &'a str) -> Self {
        HttpRequest {
            method: Method::Delete,
            url,
            user,
            token,
            payload: None,
        }
    }
}

/// An HTTP answer: any status counts as an answer, only transport failures are errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Sends one request. Production uses [`UreqTransport`]; tests pass a fake.
pub trait HttpTransport: Send + Sync {
    fn send(&self, ctx: &Ctx, request: &HttpRequest<'_>) -> Result<HttpResponse>;
}

#[cfg(test)]
mod tests;
