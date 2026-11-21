//! Generate text: rewrite a prompt with the AI tool and model chosen in Settings, streamed as NDJSON.

use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

use kiki_aitool::AiTool;
use kiki_console as console;
use kiki_console::{GenerateEvent, GenerateRequest};

use super::Server;
use crate::api_error::{ApiError, OrStatus};
use crate::controller::Controller;
use crate::http::{json_bytes, read_json, write_json, Response, Router};

/// Pattern: Controller. The Generate text routes.
pub(crate) struct GenerateController;

impl Controller for GenerateController {
    fn register(router: &mut Router<Server>) {
        router.api("POST /api/generate/preview", |server, req| {
            let body: GenerateBody = read_json(req)?;
            let (tool, model) = server.generate_choice().map_err(|e| choice_error(&e))?;
            let request = body.request(tool, &model);
            let command = console::preview_command(
                tool,
                &model,
                request.kind,
                request.subject,
                request.text,
                request.context,
            )
            .or_status(400)?;
            Ok(write_json(
                200,
                &json!({ "command": command, "tool": tool.id(), "model": model }),
            ))
        });
        router.api("POST /api/generate", |server, req| {
            let body: GenerateBody = read_json(req)?;
            console::rewrite_prompt(body.kind.trim(), &body.subject, &body.text, body.context()).or_status(400)?;
            let (tool, model) = server.generate_choice().map_err(|e| choice_error(&e))?;
            let ctx = req.ctx.with_timeout(Duration::from_secs(120));
            let reply = Response::stream(200, "application/x-ndjson", move |w: &mut dyn Write| {
                stream_rewrite(w, &ctx, &body.request(tool, &model));
                ctx.cancel();
            });
            Ok(reply.with_header("Cache-Control", "no-store"))
        });
    }
}

/// The body of a Generate text call.
#[derive(Deserialize, Default)]
#[serde(default)]
struct GenerateBody {
    kind: String,
    subject: String,
    text: String,
    prompt: String,
    agent: String,
    peer: String,
    when: String,
    send: String,
}

impl GenerateBody {
    fn context(&self) -> console::RewriteContext<'_> {
        console::RewriteContext {
            prompt: &self.prompt,
            agent: &self.agent,
            peer: &self.peer,
            when: &self.when,
            send: &self.send,
        }
    }

    fn request<'a>(&'a self, tool: AiTool, model: &'a str) -> GenerateRequest<'a> {
        GenerateRequest {
            tool,
            model,
            kind: self.kind.trim(),
            subject: &self.subject,
            text: &self.text,
            context: self.context(),
        }
    }
}

/// Writes a `meta` event, then every event of the rewrite, then an `error` event if it failed.
fn stream_rewrite(w: &mut dyn Write, ctx: &kiki_util::Ctx, request: &GenerateRequest) {
    let meta = GenerateEvent {
        r#type: "meta".into(),
        tool: request.tool.id().to_string(),
        model: request.model.to_string(),
        ..Default::default()
    };
    if write_event(w, &meta).is_err() {
        return;
    }
    let result = {
        let sink: &mut dyn Write = w;
        let mut emit = |event: GenerateEvent| -> Result<()> {
            write_event(sink, &event)?;
            Ok(())
        };
        console::generate_stream(ctx, request, &mut emit)
    };
    if let Err(err) = result {
        let event = GenerateEvent {
            r#type: "error".into(),
            error: format!("{err:#}"),
            ..Default::default()
        };
        let _ = write_event(w, &event);
    }
}

/// One NDJSON line, flushed to the client.
fn write_event(w: &mut dyn Write, event: &GenerateEvent) -> std::io::Result<()> {
    let mut line = json_bytes(event);
    line.push(b'\n');
    w.write_all(&line)?;
    w.flush()
}

/// Bad input is a 400, anything else a 500.
fn choice_error(err: &anyhow::Error) -> ApiError {
    let status = if err
        .chain()
        .any(|cause| cause.downcast_ref::<console::InputError>().is_some())
    {
        400
    } else {
        500
    };
    ApiError::from_anyhow(status, err)
}

impl Server {
    /// The AI tool and model chosen for Generate text.
    pub(crate) fn generate_choice(&self) -> Result<(AiTool, String)> {
        let snap = self.load()?;
        if snap.generate_tool.trim().is_empty() {
            return Err(anyhow::Error::new(console::InputError {
                message: "choose an AI Tool for Generate text".into(),
            }));
        }
        let tool = console::parse_tool(&snap.generate_tool)?;
        let model = console::resolve_model(&self.model_catalog(), tool, &snap.generate_model)
            .unwrap_or_else(|_| snap.generate_model.trim().to_string());
        Ok((tool, model))
    }
}
