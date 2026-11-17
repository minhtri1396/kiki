//! Reading and writing Orca terminals: the rendered screen, idleness, and text sent to the agent.
//! `prompt` handles the launch prompt that sometimes stays unsent in the agent's input.

mod prompt;
#[cfg(test)]
mod tests;

use crate::cli::Cli;
use crate::wire::{json_ok, nd, parse, Envelope};
use anyhow::{anyhow, Result};
use kiki_util::Ctx;
use serde::Deserialize;
use serde_json::Value;

/// TerminalView is the rendered screen of one Orca terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalView {
    pub handle: String,
    pub status: String,
    pub log: String,
    /// Draft is text sitting in the agent's input, not yet submitted.
    pub draft: String,
}

/// The terminal commands of one Orca CLI.
pub(crate) struct Terminals {
    cli: Cli,
}

impl Terminals {
    pub(crate) fn new(cli: Cli) -> Terminals {
        Terminals { cli }
    }

    /// Reads what an Orca terminal is showing, and whether it is still running.
    pub(crate) fn read(&self, ctx: &Ctx, handle: &str) -> Result<TerminalView> {
        let handle = require_handle(handle)?;
        let out = self.cli.command_output(
            ctx,
            &[
                "terminal",
                "read",
                "--terminal",
                handle,
                "--screen",
                "--limit",
                "200",
                "--json",
            ],
        )?;
        parse_terminal_read(&out)
    }

    /// Reports whether the terminal's text interface has stopped updating.
    pub(crate) fn idle(&self, ctx: &Ctx, handle: &str) -> Result<bool> {
        let handle = require_handle(handle)?;
        let out = self.cli.command_output(
            ctx,
            &[
                "terminal",
                "wait",
                "--terminal",
                handle,
                "--for",
                "tui-idle",
                "--timeout-ms",
                "400",
                "--json",
            ],
        )?;
        parse_terminal_idle(&out)
    }

    /// Submits a reply to the agent in an Orca terminal.
    pub(crate) fn send_text(&self, ctx: &Ctx, handle: &str, text: &str) -> Result<()> {
        let handle = handle.trim();
        let text = text.trim();
        if handle.is_empty() || text.is_empty() {
            return Err(anyhow!("Orca did not submit the prompt"));
        }
        let out = self.cli.command_output(
            ctx,
            &[
                "terminal",
                "send",
                "--terminal",
                handle,
                "--text",
                text,
                "--enter",
                "--wait-submit",
                "8",
                "--json",
            ],
        )?;
        sent_ok(&out)
    }

    fn press_enter(&self, ctx: &Ctx, handle: &str) -> Result<()> {
        let out = self.cli.command_output(
            ctx,
            &[
                "terminal",
                "send",
                "--terminal",
                handle,
                "--enter",
                "--wait-submit",
                "8",
                "--json",
            ],
        )?;
        sent_ok(&out)
    }
}

fn require_handle(handle: &str) -> Result<&str> {
    let handle = handle.trim();
    if handle.is_empty() {
        return Err(anyhow!("terminal handle is required"));
    }
    Ok(handle)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TerminalBody {
    #[serde(deserialize_with = "nd")]
    handle: String,
    #[serde(deserialize_with = "nd")]
    status: String,
    #[serde(deserialize_with = "nd")]
    tail: Vec<String>,
    draft: Value,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ReadResult {
    #[serde(deserialize_with = "nd")]
    terminal: TerminalBody,
}

pub(crate) fn parse_terminal_read(out: &[u8]) -> Result<TerminalView> {
    let env: Envelope<ReadResult> = parse(out).map_err(|e| anyhow!("read Orca terminal: {e}"))?;
    if !env.ok {
        return Err(anyhow!(
            "{}",
            message_or(&env.error.message, "Orca did not return the terminal")
        ));
    }
    let terminal = env.result.terminal;
    let status = match terminal.status.trim() {
        status @ ("running" | "exited" | "unknown") => status.to_string(),
        _ => "unknown".to_string(),
    };
    Ok(TerminalView {
        handle: terminal.handle,
        status,
        log: terminal.tail.join("\n").trim_end_matches('\n').to_string(),
        draft: draft_text_value(&terminal.draft),
    })
}

pub(crate) fn parse_terminal_idle(out: &[u8]) -> Result<bool> {
    let env: Envelope<Value> = parse(out).map_err(|e| anyhow!("read Orca terminal: {e}"))?;
    if env.ok {
        return Ok(true);
    }
    if env.error.code == "timeout" {
        return Ok(false);
    }
    Err(anyhow!(
        "{}",
        message_or(&env.error.message, "Orca did not report whether the terminal is idle")
    ))
}

/// The trimmed message, or `fallback` when it is blank.
fn message_or(message: &str, fallback: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        fallback.to_string()
    } else {
        message.to_string()
    }
}

/// The text of an agent's input draft: a string, or the first non-blank of `text`, `value`, `content`.
pub(crate) fn draft_text_value(raw: &Value) -> String {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Draft {
        #[serde(deserialize_with = "nd")]
        text: String,
        #[serde(deserialize_with = "nd")]
        value: String,
        #[serde(deserialize_with = "nd")]
        content: String,
    }
    match raw {
        Value::String(text) => text.clone(),
        Value::Object(_) => serde_json::from_value::<Draft>(raw.clone())
            .ok()
            .and_then(|d| {
                [d.text, d.value, d.content]
                    .into_iter()
                    .find(|part| !part.trim().is_empty())
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Accepts a send reply: ok, or a timeout (the agent is just slow to take the input).
fn sent_ok(out: &[u8]) -> Result<()> {
    if json_ok(out) {
        return Ok(());
    }
    let env: Envelope<Value> = parse(out).unwrap_or_default();
    if env.error.code == "timeout" {
        return Ok(());
    }
    Err(anyhow!(
        "{}",
        message_or(&env.error.message, "Orca did not submit the prompt")
    ))
}

/// TerminalHandle is the Orca terminal from an agent launch response.
pub fn terminal_handle(out: &str) -> String {
    terminal_handle_bytes(out.as_bytes())
}

pub(crate) fn terminal_handle_bytes(out: &[u8]) -> String {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Outcome {
        #[serde(deserialize_with = "nd")]
        handle: String,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Launched {
        #[serde(deserialize_with = "nd")]
        outcome: Outcome,
    }
    parse::<Envelope<Launched>>(out)
        .map(|env| env.result.outcome.handle)
        .unwrap_or_default()
}
