//! Rewriting text with an AI tool's CLI.
//!
//! Pattern: Template Method. `run_generate` is the one skeleton (plan the command, start the process,
//! read its output, map the result); `generate_text` and `generate_stream` differ only in the
//! `GenerateSink` that receives the events. Per-tool command lines are a Strategy in `strategy`.

mod events;
mod rewrite;
mod strategy;
mod stream;

#[cfg(test)]
mod tests;

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use kiki_aitool::AiTool;
use kiki_util::{Ctx, CtxErr};

use crate::diff::{text_diff, DiffLine};
use crate::host::{CliHost, CommandRun, SystemHost};
use crate::text::{one_line, truncate_bytes};

pub use rewrite::{rewrite_prompt, RewriteContext};
pub use strategy::{format_generate_command, generate_args, preview_command};

use strategy::add_workspace;
use stream::{clean_generated, read_output, Outcome};

/// How long a rewrite may run when the caller sets no deadline.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2 * 60);

/// One chunk of a rewrite. Log events are the model output as it
/// arrives. Done carries the rewritten text and a git-style diff against the original.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateEvent {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "channel", default, skip_serializing_if = "String::is_empty")]
    pub channel: String,
    #[serde(rename = "text", default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(rename = "tool", default, skip_serializing_if = "String::is_empty")]
    pub tool: String,
    #[serde(rename = "model", default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(rename = "command", default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(rename = "diff", default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<DiffLine>,
    #[serde(rename = "error", default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// One rewrite: which tool and model, what kind of text it is, and the text.
#[derive(Debug, Clone, Copy)]
pub struct GenerateRequest<'a> {
    pub tool: AiTool,
    pub model: &'a str,
    pub kind: &'a str,
    pub subject: &'a str,
    pub text: &'a str,
    pub context: RewriteContext<'a>,
}

/// Where the events of a rewrite go. The hooks are the variable steps of `run_generate`.
pub(crate) trait GenerateSink {
    /// The command line about to run.
    fn command(&mut self, command: String) -> Result<()>;
    /// One log event. An error stops the reader that produced it.
    fn log(&mut self, event: GenerateEvent) -> Result<()>;
    /// The rewritten text and its diff against the original.
    fn done(&mut self, text: String, diff: Vec<DiffLine>) -> Result<()>;
}

/// Forwards every event to a caller's closure.
struct EmitSink<'a>(&'a mut dyn FnMut(GenerateEvent) -> Result<()>);

impl GenerateSink for EmitSink<'_> {
    fn command(&mut self, command: String) -> Result<()> {
        (self.0)(GenerateEvent {
            r#type: "command".into(),
            command,
            ..Default::default()
        })
    }

    fn log(&mut self, event: GenerateEvent) -> Result<()> {
        (self.0)(event)
    }

    fn done(&mut self, text: String, diff: Vec<DiffLine>) -> Result<()> {
        (self.0)(GenerateEvent {
            r#type: "done".into(),
            text,
            diff,
            ..Default::default()
        })
    }
}

/// Keeps only the rewritten text.
#[derive(Default)]
struct TextSink {
    text: String,
}

impl GenerateSink for TextSink {
    fn command(&mut self, _command: String) -> Result<()> {
        Ok(())
    }

    fn log(&mut self, _event: GenerateEvent) -> Result<()> {
        Ok(())
    }

    fn done(&mut self, text: String, _diff: Vec<DiffLine>) -> Result<()> {
        self.text = text;
        Ok(())
    }
}

/// Rewrites text with the AI tool's CLI and returns the final text.
pub fn generate_text(ctx: &Ctx, req: &GenerateRequest) -> Result<String> {
    generate_text_with(&SystemHost, ctx, req)
}

/// `generate_text` on a given host.
pub fn generate_text_with(host: &dyn CliHost, ctx: &Ctx, req: &GenerateRequest) -> Result<String> {
    let mut sink = TextSink::default();
    run_generate(host, ctx, req, &mut sink)?;
    if sink.text.is_empty() {
        return Err(anyhow!("the model returned no text"));
    }
    Ok(sink.text)
}

/// Runs the CLI and reports log lines, then the rewritten text.
pub fn generate_stream(
    ctx: &Ctx,
    req: &GenerateRequest,
    emit: &mut dyn FnMut(GenerateEvent) -> Result<()>,
) -> Result<()> {
    generate_stream_with(&SystemHost, ctx, req, emit)
}

/// `generate_stream` on a given host.
pub fn generate_stream_with(
    host: &dyn CliHost,
    ctx: &Ctx,
    req: &GenerateRequest,
    emit: &mut dyn FnMut(GenerateEvent) -> Result<()>,
) -> Result<()> {
    run_generate(host, ctx, req, &mut EmitSink(emit))
}

/// The template method: plan, start, read, finish.
fn run_generate(host: &dyn CliHost, ctx: &Ctx, req: &GenerateRequest, sink: &mut dyn GenerateSink) -> Result<()> {
    let mut plan = Plan::build(host, req)?;
    // The folder is removed when `tmp` drops.
    let tmp = tempfile::Builder::new().prefix("kiki-generate-").tempdir()?;
    let dir = tmp.path().to_string_lossy().into_owned();
    add_workspace(plan.tool, &mut plan.args, &dir);
    sink.command(format_generate_command(&plan.bin, &plan.args, &plan.prompt))?;
    let run = plan.start(host, ctx, &dir)?;
    let outcome = read_output(run, sink);
    plan.finish(outcome, sink)
}

/// The command a rewrite will run, before it starts.
struct Plan {
    tool: AiTool,
    original: String,
    prompt: String,
    bin: String,
    args: Vec<String>,
}

impl Plan {
    /// Builds the prompt and the command line, and checks the CLI is installed.
    fn build(host: &dyn CliHost, req: &GenerateRequest) -> Result<Plan> {
        let original = req.text.trim();
        let prompt = rewrite_prompt(req.kind, req.subject, original, req.context)?;
        let (bin, args) = generate_args(req.tool, req.model, &prompt)?;
        if !host.command_exists(&bin) {
            return Err(anyhow!("{} is not installed", bin));
        }
        Ok(Plan {
            tool: req.tool,
            original: original.to_string(),
            prompt,
            bin,
            args,
        })
    }

    /// Starts the CLI in `dir`, with a deadline when the caller has none.
    fn start(&self, host: &dyn CliHost, ctx: &Ctx, dir: &str) -> Result<CommandRun> {
        let ctx = if ctx.remaining().is_none() {
            ctx.with_timeout(DEFAULT_TIMEOUT)
        } else {
            ctx.clone()
        };
        host.start(&ctx, dir, &self.bin, &self.args)
            .map_err(|err| self.timeout_or(err))
    }

    /// "took too long" for a deadline error, the error itself otherwise.
    fn timeout_or(&self, err: anyhow::Error) -> anyhow::Error {
        if is_deadline(&err) {
            return anyhow!("{} took too long", self.bin);
        }
        err
    }

    /// Turns what the CLI printed and how it ended into the rewritten text or an error.
    fn finish(&self, outcome: Outcome, sink: &mut dyn GenerateSink) -> Result<()> {
        let Outcome { state, wait_err } = outcome;
        if state.failed {
            let message = if state.fail_message.is_empty() {
                "the model failed"
            } else {
                state.fail_message.as_str()
            };
            return Err(anyhow!("{}", message));
        }
        if let Some(wait_err) = &wait_err {
            if state.best_text().is_empty() {
                if is_deadline(wait_err) {
                    return Err(anyhow!("{} took too long", self.bin));
                }
                let message = truncate_bytes(&one_line(&format!("{wait_err:#}")), 300);
                return Err(anyhow!("{}: {}", self.bin, message));
            }
        }
        let final_text = clean_generated(state.best_text());
        if final_text.is_empty() {
            return Err(anyhow!("{} returned no text", self.bin));
        }
        let diff = text_diff(&self.original, &final_text);
        sink.done(final_text, diff)
    }
}

fn is_deadline(err: &anyhow::Error) -> bool {
    matches!(err.downcast_ref::<CtxErr>(), Some(CtxErr::DeadlineExceeded))
}
