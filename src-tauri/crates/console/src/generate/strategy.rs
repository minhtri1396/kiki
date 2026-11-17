//! Pattern: Strategy + Factory Method. One `GenerateStrategy` per AI tool builds the command line that
//! runs the tool for a one-shot text rewrite; `generate_strategy` picks it for an `AiTool`.

use anyhow::Result;

use kiki_aitool::AiTool;

use crate::error::input_error;
use crate::tool::MODEL_ID;

use super::rewrite::rewrite_prompt;

/// Stands in for the temporary folder in the command shown
/// before Run. The real run replaces it with the folder path.
const PREVIEW_WORKSPACE: &str = "kiki-preview-workspace";

/// How one AI tool is run to rewrite text.
pub(crate) trait GenerateStrategy: Sync {
    /// The arguments after the program name. An empty `model` leaves the tool's own default.
    fn args(&self, model: &str, prompt: &str) -> Vec<String>;

    /// Whether the tool wants `--workspace <folder>` before the prompt.
    fn takes_workspace(&self) -> bool {
        false
    }
}

/// The strategy that runs `tool`.
pub(crate) fn generate_strategy(tool: AiTool) -> &'static dyn GenerateStrategy {
    match tool {
        AiTool::Cursor => &CursorStrategy,
        AiTool::Claude => &ClaudeStrategy,
        AiTool::Codex => &CodexStrategy,
        AiTool::Gemini => &GeminiStrategy,
    }
}

/// `fixed`, then `--model <model>` when a model is set, then the prompt as the last argument.
fn fixed_model_prompt(fixed: &[&str], model: &str, prompt: &str) -> Vec<String> {
    let mut args: Vec<String> = fixed.iter().map(|arg| arg.to_string()).collect();
    if !model.is_empty() {
        args.push("--model".into());
        args.push(model.into());
    }
    args.push(prompt.into());
    args
}

struct CursorStrategy;

impl GenerateStrategy for CursorStrategy {
    fn args(&self, model: &str, prompt: &str) -> Vec<String> {
        fixed_model_prompt(
            &[
                "--print",
                "--output-format",
                "stream-json",
                "--stream-partial-output",
                "--mode",
                "ask",
                "--trust",
            ],
            model,
            prompt,
        )
    }

    fn takes_workspace(&self) -> bool {
        true
    }
}

struct ClaudeStrategy;

impl GenerateStrategy for ClaudeStrategy {
    fn args(&self, model: &str, prompt: &str) -> Vec<String> {
        fixed_model_prompt(
            &[
                "--print",
                "--bare",
                "--verbose",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--tools",
                "",
                "--no-session-persistence",
            ],
            model,
            prompt,
        )
    }
}

struct CodexStrategy;

impl GenerateStrategy for CodexStrategy {
    fn args(&self, model: &str, prompt: &str) -> Vec<String> {
        fixed_model_prompt(
            &["exec", "--skip-git-repo-check", "--sandbox", "read-only"],
            model,
            prompt,
        )
    }
}

/// Gemini takes the prompt with `--prompt`, not as the last argument.
struct GeminiStrategy;

impl GenerateStrategy for GeminiStrategy {
    fn args(&self, model: &str, prompt: &str) -> Vec<String> {
        let mut args = vec!["--prompt".to_string(), prompt.to_string()];
        if !model.is_empty() {
            args.push("--model".into());
            args.push(model.into());
        }
        args
    }
}

/// The CLI argv for a one-shot rewrite, as `(bin, args)`. The prompt is the last argument,
/// except for Gemini, which takes it with --prompt.
pub fn generate_args(tool: AiTool, model: &str, prompt: &str) -> Result<(String, Vec<String>)> {
    let model = model.trim();
    if !model.is_empty() && !MODEL_ID.is_match(model) {
        return Err(input_error("Model is not supported"));
    }
    Ok((tool.binary().to_string(), generate_strategy(tool).args(model, prompt)))
}

/// Puts `--workspace <dir>` before the prompt for the tools that take it.
pub(crate) fn add_workspace(tool: AiTool, args: &mut Vec<String>, dir: &str) {
    if !generate_strategy(tool).takes_workspace() {
        return;
    }
    let last = args.pop().unwrap_or_default();
    args.push("--workspace".to_string());
    args.push(dir.to_string());
    args.push(last);
}

/// The CLI command for a rewrite, with {{prompt}} where the
/// prompt is passed. Cursor also shows {{workspace}} for the temporary folder.
pub fn preview_command(tool: AiTool, model: &str, kind: &str, subject: &str, text: &str) -> Result<String> {
    let prompt = rewrite_prompt(kind, subject, text)?;
    let (bin, mut args) = generate_args(tool, model, &prompt)?;
    add_workspace(tool, &mut args, PREVIEW_WORKSPACE);
    Ok(format_generate_command(&bin, &args, &prompt))
}

/// The command line shown to the user. The prompt argument
/// is {{prompt}}, so the dialog does not print the prompt itself.
pub fn format_generate_command(bin: &str, args: &[String], prompt: &str) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(1 + args.len());
    parts.push(quote_command_arg(bin));
    for arg in args {
        if !prompt.is_empty() && arg == prompt {
            parts.push("{{prompt}}".to_string());
        } else if arg == PREVIEW_WORKSPACE {
            parts.push("{{workspace}}".to_string());
        } else {
            parts.push(quote_command_arg(arg));
        }
    }
    parts.join(" ")
}

fn quote_command_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if arg.contains([
        ' ', '\t', '\n', '"', '\'', '\\', '$', '`', '<', '>', '|', '&', ';', '(', ')',
    ]) {
        return format!("'{}'", arg.replace('\'', r"'\''"));
    }
    arg.to_string()
}
