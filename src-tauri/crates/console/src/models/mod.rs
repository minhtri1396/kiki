//! The models each AI tool can launch.
//!
//! Pattern: Strategy + Factory Method. One `ModelSource` per AI tool knows its built-in fallback list and
//! where its live list comes from (the CLI, or the Claude Code docs); `model_source` picks it for an `AiTool`.

mod parse;
mod source;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io::Read;
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use kiki_aitool::AiTool;
use kiki_util::{run_output, Ctx};

use crate::host::{CliHost, SystemHost};

pub use parse::{parse_claude_model_doc, parse_cursor_models};
use source::model_source;

/// One provider model for an AI tool.
/// `group` is the dropdown section, used for Cursor's Cursor Models and Other Models.
///
/// The JSON keys are capitalised (`ID`, `Label`, ...) because the web UI and saved files already use them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Model {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Label")]
    pub label: String,
    #[serde(rename = "Default")]
    pub default: bool,
    #[serde(rename = "Group")]
    pub group: String,
}

pub(crate) fn model(id: &str, label: &str, default: bool, group: &str) -> Model {
    Model {
        id: id.into(),
        label: label.into(),
        default,
        group: group.into(),
    }
}

/// `ORCA_AGENTS` limited to tools whose CLI is on PATH.
pub fn installed_agents() -> Vec<String> {
    installed_agents_with(&SystemHost)
}

/// `installed_agents` on a given host.
pub fn installed_agents_with(host: &dyn CliHost) -> Vec<String> {
    AiTool::ALL
        .into_iter()
        .filter(|tool| host.command_exists(tool.binary()))
        .map(|tool| tool.id().to_string())
        .collect()
}

/// The model list Orca uses for each AI tool.
/// Cursor is a short fallback. The running app replaces it with `cursor-agent models` when that command succeeds.
/// Claude labels are the versions those aliases resolve to on the Anthropic API.
/// The running app replaces them with the current Claude Code model config when that page can be read.
/// The id stays the alias, which is what `orca agent launch --model` accepts.
pub fn model_catalog() -> BTreeMap<String, Vec<Model>> {
    AiTool::ALL
        .into_iter()
        .map(|tool| (tool.id().to_string(), model_source(tool).fallback()))
        .collect()
}

/// Checks a model id for an AI tool. An empty model uses that tool's default.
pub fn resolve_model(catalog: &BTreeMap<String, Vec<Model>>, tool: AiTool, model: &str) -> Result<String> {
    let model = model.trim();
    let models: &[Model] = catalog.get(tool.id()).map(|v| v.as_slice()).unwrap_or(&[]);
    if model.is_empty() {
        return models
            .iter()
            .find(|item| item.default)
            .or_else(|| models.first())
            .map(|item| item.id.clone())
            .ok_or_else(|| anyhow!("model is not supported"));
    }
    models
        .iter()
        .find(|item| item.id == model)
        .map(|item| item.id.clone())
        .ok_or_else(|| anyhow!("model is not supported"))
}

/// Asks the AI tool's own CLI for the models it can launch.
/// Claude has no model-list command, so that tool uses the Claude Code model config.
pub fn live_models(ctx: &Ctx, tool: AiTool) -> Vec<Model> {
    live_models_with(&SystemHost, ctx, tool)
}

/// `live_models` on a given host.
pub fn live_models_with(host: &dyn CliHost, ctx: &Ctx, tool: AiTool) -> Vec<Model> {
    model_source(tool).live(host, ctx)
}

/// A context with no deadline gets `default`.
fn with_default_timeout(ctx: &Ctx, default: Duration) -> Ctx {
    if ctx.remaining().is_none() {
        ctx.with_timeout(default)
    } else {
        ctx.clone()
    }
}

/// Runs `<bin> models` and reads its list. Any failure is an empty list.
fn cli_models(ctx: &Ctx, bin: &str) -> Vec<Model> {
    let ctx = with_default_timeout(ctx, Duration::from_secs(15));
    let Ok(out) = run_output(&ctx, Command::new(bin).arg("models")) else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_cursor_models(&String::from_utf8_lossy(&out.stdout))
}

/// Asks the local Cursor CLI which models it can launch.
pub fn cursor_models(ctx: &Ctx) -> Vec<Model> {
    cli_models(ctx, AiTool::Cursor.binary())
}

const CLAUDE_MODEL_DOC_URL: &str = "https://code.claude.com/docs/en/model-config.md";

/// Reads the current alias versions from the Claude Code model config.
/// A network or parse failure returns nothing so the caller keeps `model_catalog`.
pub fn claude_models(ctx: &Ctx) -> Vec<Model> {
    let ctx = with_default_timeout(ctx, Duration::from_secs(4));
    if ctx.done() {
        return Vec::new();
    }
    let timeout = ctx.remaining().unwrap_or(Duration::from_secs(4));
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let resp = match agent
        .get(CLAUDE_MODEL_DOC_URL)
        .set("Accept", "text/plain, text/markdown")
        .call()
    {
        Ok(resp) => resp,
        Err(_) => return Vec::new(),
    };
    if resp.status() != 200 {
        return Vec::new();
    }
    let mut body = Vec::new();
    if resp.into_reader().take(1 << 20).read_to_end(&mut body).is_err() {
        return Vec::new();
    }
    parse_claude_model_doc(&String::from_utf8_lossy(&body))
}
