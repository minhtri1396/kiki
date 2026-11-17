//! The models each AI tool can launch: a cache that refreshes from the tools' CLIs, and the models route.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use kiki_aitool::AiTool;
use kiki_console as console;
use kiki_console::Model;
use kiki_util::Ctx;

use super::Server;
use crate::api_error::OrStatus;
use crate::controller::Controller;
use crate::http::{write_json, Router};

/// How long a model list is reused before the CLIs are asked again, until Cursor has answered once.
const REFRESH_AFTER: Duration = Duration::from_secs(15);

/// The model lists and which of them came from a CLI.
#[derive(Default)]
pub struct ModelsState {
    pub models: Option<BTreeMap<String, Vec<Model>>>,
    pub at: Option<Instant>,
    pub cursor_ready: bool,
    pub claude_ready: bool,
}

/// Pattern: Controller. The models route.
pub(crate) struct ModelController;

impl Controller for ModelController {
    fn register(router: &mut Router<Server>) {
        router.api("GET /api/tools/{tool}/models", |server, req| {
            let tool = console::parse_tool(req.path_value("tool")).or_status(400)?;
            let ctx = req.ctx.with_timeout(Duration::from_secs(20));
            let mut models = console::live_models(&ctx, tool);
            ctx.cancel();
            if models.is_empty() {
                models = console::model_catalog().get(tool.id()).cloned().unwrap_or_default();
            }
            server.store_tool_models(tool, &models);
            Ok(write_json(200, &json!({ "models": model_rows(&models) })))
        });
    }
}

impl Server {
    /// Remembers a list read from a CLI.
    fn store_tool_models(&self, tool: AiTool, models: &[Model]) {
        let mut st = self.models.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = st.models.clone().unwrap_or_else(console::model_catalog);
        next.insert(tool.id().to_string(), models.to_vec());
        st.models = Some(next);
        st.at = Some(Instant::now());
        match tool {
            AiTool::Cursor => st.cursor_ready = true,
            AiTool::Claude => st.claude_ready = true,
            AiTool::Codex | AiTool::Gemini => {}
        }
    }

    /// The cached model lists, refreshed from the CLIs at most every 15 seconds until the Cursor list has been
    /// read once.
    pub(crate) fn model_catalog(&self) -> BTreeMap<String, Vec<Model>> {
        let (claude_ready, previous_claude) = match self.cached_models() {
            Ok(cached) => return cached,
            Err(state) => state,
        };
        let mut claude_ready = claude_ready;
        let mut catalog = console::model_catalog();
        if claude_ready && !previous_claude.is_empty() {
            catalog.insert("claude".to_string(), previous_claude);
        } else {
            let live = console::claude_models(&Ctx::background());
            if !live.is_empty() {
                catalog.insert("claude".to_string(), live);
                claude_ready = true;
            }
        }
        let live = console::cursor_models(&Ctx::background());
        let cursor_ready = !live.is_empty();
        if cursor_ready {
            catalog.insert("cursor".to_string(), live);
        }

        let mut st = self.models.lock().unwrap_or_else(|e| e.into_inner());
        if st.cursor_ready {
            if let Some(models) = &st.models {
                return models.clone();
            }
        }
        st.models = Some(catalog);
        st.at = Some(Instant::now());
        st.cursor_ready = cursor_ready;
        if claude_ready {
            st.claude_ready = true;
        }
        st.models.clone().unwrap_or_default()
    }

    /// The lists to reuse (`Ok`), or what a refresh starts from: whether Claude was read before, and its last list.
    fn cached_models(&self) -> Result<BTreeMap<String, Vec<Model>>, (bool, Vec<Model>)> {
        let st = self.models.lock().unwrap_or_else(|e| e.into_inner());
        if let (true, Some(models)) = (st.cursor_ready, &st.models) {
            return Ok(models.clone());
        }
        if let (Some(models), Some(at)) = (&st.models, st.at) {
            if at.elapsed() < REFRESH_AFTER {
                return Ok(models.clone());
            }
        }
        let previous = st
            .models
            .as_ref()
            .and_then(|m| m.get("claude"))
            .cloned()
            .unwrap_or_default();
        Err((st.claude_ready, previous))
    }
}

fn model_rows(models: &[Model]) -> Vec<Value> {
    models
        .iter()
        .map(|model| {
            let mut row = Map::new();
            row.insert("id".into(), Value::String(model.id.clone()));
            row.insert("label".into(), Value::String(model.label.clone()));
            row.insert("default".into(), Value::Bool(model.default));
            if !model.group.is_empty() {
                row.insert("group".into(), Value::String(model.group.clone()));
            }
            Value::Object(row)
        })
        .collect()
}

/// The model lists of every AI tool, for the state document.
pub(crate) fn model_json(catalog: &BTreeMap<String, Vec<Model>>) -> Value {
    let mut out = Map::new();
    for tool in AiTool::ALL {
        let rows = catalog.get(tool.id()).map(|list| model_rows(list)).unwrap_or_default();
        out.insert(tool.id().to_string(), Value::Array(rows));
    }
    Value::Object(out)
}
