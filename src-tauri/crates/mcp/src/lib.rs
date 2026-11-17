//! Installs the Atlassian MCP server into an AI tool and talks to Jira and Confluence.
//!
//! - [`Atlassian`] / [`Service`]: the saved setup, its validation, and the environment `mcp-atlassian` reads.
//! - [`AtlassianApi`]: Facade over Jira and Confluence calls (boards, spaces, pages, epics) on an injected
//!   [`HttpTransport`].
//! - [`ensure`] / [`Installer`]: writes the MCP server into each AI tool's settings.

mod api;
mod atlassian;
mod http;
mod install;
mod json_object;
#[cfg(test)]
mod testutil;
mod urlenc;
mod wire;

pub use api::{AtlassianApi, Epic, Hit};
pub use atlassian::{apply, Atlassian, Service};
pub use http::{HttpRequest, HttpResponse, HttpTransport, Method, UreqTransport};
pub use install::{ensure, server_key, Installer, LookPath};
