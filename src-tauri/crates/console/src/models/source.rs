//! Where each AI tool's model list comes from.

use kiki_aitool::AiTool;
use kiki_util::Ctx;

use crate::host::CliHost;

use super::parse::{CURSOR_MODELS_GROUP, OTHER_MODELS_GROUP};
use super::{claude_models, cli_models, cursor_models, model, Model};

/// One AI tool's model list.
pub(crate) trait ModelSource: Sync {
    /// The built-in list, used until (or unless) the live list can be read.
    fn fallback(&self) -> Vec<Model>;

    /// The list the tool itself reports now; empty when it cannot be read.
    fn live(&self, host: &dyn CliHost, ctx: &Ctx) -> Vec<Model>;
}

/// The source for `tool`.
pub(crate) fn model_source(tool: AiTool) -> &'static dyn ModelSource {
    match tool {
        AiTool::Cursor => &CursorModels,
        AiTool::Claude => &ClaudeModels,
        AiTool::Codex => &CodexModels,
        AiTool::Gemini => &GeminiModels,
    }
}

/// `cursor-agent models`.
struct CursorModels;

impl ModelSource for CursorModels {
    fn fallback(&self) -> Vec<Model> {
        vec![
            model("auto", "Auto", true, CURSOR_MODELS_GROUP),
            model("composer-2.5", "Composer 2.5", false, CURSOR_MODELS_GROUP),
            model("gpt-5.3-codex", "GPT-5.3 Codex", false, OTHER_MODELS_GROUP),
            model("claude-opus-4-8", "Claude Opus 4.8", false, OTHER_MODELS_GROUP),
        ]
    }

    fn live(&self, _host: &dyn CliHost, ctx: &Ctx) -> Vec<Model> {
        cursor_models(ctx)
    }
}

/// Claude has no model-list command: the Claude Code model config page lists the aliases.
struct ClaudeModels;

impl ModelSource for ClaudeModels {
    fn fallback(&self) -> Vec<Model> {
        vec![
            model("fable", "Fable 5.2", false, ""),
            model("opus", "Opus 5.5", false, ""),
            model("sonnet", "Sonnet 5.5", true, ""),
            model("haiku", "Haiku 5.5", false, ""),
        ]
    }

    fn live(&self, _host: &dyn CliHost, ctx: &Ctx) -> Vec<Model> {
        claude_models(ctx)
    }
}

/// `codex models`, when the CLI is installed.
struct CodexModels;

impl ModelSource for CodexModels {
    fn fallback(&self) -> Vec<Model> {
        vec![
            model("gpt-5.6-sol", "GPT-5.6 Sol", true, ""),
            model("gpt-5.6-terra", "GPT-5.6 Terra", false, ""),
            model("gpt-5.6-luna", "GPT-5.6 Luna", false, ""),
            model("gpt-5.5", "GPT-5.5", false, ""),
            model("gpt-5.2-codex", "GPT-5.2 Codex", false, ""),
        ]
    }

    fn live(&self, host: &dyn CliHost, ctx: &Ctx) -> Vec<Model> {
        installed_cli_models(host, ctx, AiTool::Codex)
    }
}

/// `gemini models`, when the CLI is installed.
struct GeminiModels;

impl ModelSource for GeminiModels {
    fn fallback(&self) -> Vec<Model> {
        vec![
            model("gemini-3-pro-preview", "Gemini 3 Pro Preview", true, ""),
            model("gemini-3-flash-preview", "Gemini 3 Flash Preview", false, ""),
            model("gemini-2.5-pro", "Gemini 2.5 Pro", false, ""),
            model("gemini-2.5-flash", "Gemini 2.5 Flash", false, ""),
        ]
    }

    fn live(&self, host: &dyn CliHost, ctx: &Ctx) -> Vec<Model> {
        installed_cli_models(host, ctx, AiTool::Gemini)
    }
}

fn installed_cli_models(host: &dyn CliHost, ctx: &Ctx, tool: AiTool) -> Vec<Model> {
    if !host.command_exists(tool.binary()) {
        return Vec::new();
    }
    cli_models(ctx, tool.binary())
}
