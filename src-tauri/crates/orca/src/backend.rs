//! The two ways to reach Orca: its command line and its runtime. Every collaborator of the client holds one.

use crate::cli::Cli;
use crate::transport::RuntimeTransport;
use crate::Client;
use anyhow::Result;
use kiki_util::Ctx;
use serde_json::Value;
use std::sync::Arc;

/// The CLI and the runtime transport of one client.
#[derive(Clone)]
pub(crate) struct Backend {
    pub(crate) cli: Cli,
    transport: Arc<dyn RuntimeTransport>,
}

impl Backend {
    pub(crate) fn new(client: &Client) -> Backend {
        Backend {
            cli: Cli::from(client),
            transport: client.transport(),
        }
    }

    /// Calls a runtime method (see [`RuntimeTransport`]).
    pub(crate) fn runtime(&self, ctx: &Ctx, method: &str, params: Option<&Value>) -> Result<Value> {
        self.transport.call(ctx, method, params)
    }
}
