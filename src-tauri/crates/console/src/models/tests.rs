use std::collections::BTreeMap;

use super::*;
use crate::testing::FakeHost;

fn catalog_of(tool: AiTool, models: Vec<Model>) -> BTreeMap<String, Vec<Model>> {
    BTreeMap::from([(tool.id().to_string(), models)])
}

#[test]
fn parse_cursor_models_groups() {
    let text = "
Available models

auto - Auto (default)
composer-2.5 - Composer 2.5
cursor-grok-4.5-high - Grok 4.5
grok-4.7-high - Grok 4.7  High
gpt-5.3-codex - Codex 5.3
claude-opus-4-8-high - Claude Opus 4.8 1M (current)

Tip: use --model <id> to switch.
";
    let models = parse_cursor_models(text);
    assert_eq!(models.len(), 6, "got {} models", models.len());
    let want = [
        model("auto", "Auto", true, "Cursor Models"),
        model("composer-2.5", "Composer 2.5", false, "Cursor Models"),
        model("cursor-grok-4.5-high", "Grok 4.5", false, "Cursor Models"),
        model("grok-4.7-high", "Grok 4.7  High", false, "Cursor Models"),
        model("gpt-5.3-codex", "Codex 5.3", false, "Other Models"),
        model("claude-opus-4-8-high", "Claude Opus 4.8 1M", false, "Other Models"),
    ];
    for (i, want) in want.iter().enumerate() {
        assert_eq!(&models[i], want, "model {i}");
    }
}

#[test]
fn parse_cursor_models_defaults_to_the_first_and_skips_repeats() {
    let models = parse_cursor_models("a - Alpha\nb c - Spaced id\na - Again\nb - Beta (x)\n");
    assert_eq!(
        models,
        vec![
            model("a", "Alpha", true, "Other Models"),
            model("b", "Beta", false, "Other Models")
        ]
    );
    assert!(parse_cursor_models("no models here").is_empty());
}

#[test]
fn parse_claude_model_doc_reads_alias_row() {
    let text = "
| Provider | `opus` | `sonnet` | `haiku` |
| Anthropic API | Opus 5.5 | Sonnet 5.5 | Haiku 5.5 |
| Microsoft Foundry | Opus 4.6 | Sonnet 4.5 | Haiku 4.5 |

Unless you set it, the fable alias resolves to Fable 5 in gateway sessions and to Fable 5.2 everywhere else.
";
    let models = parse_claude_model_doc(text);
    assert_eq!(models.len(), 4, "got {} models", models.len());
    let want = [
        model("fable", "Fable 5.2", false, ""),
        model("opus", "Opus 5.5", false, ""),
        model("sonnet", "Sonnet 5.5", true, ""),
        model("haiku", "Haiku 5.5", false, ""),
    ];
    for (i, want) in want.iter().enumerate() {
        assert_eq!(&models[i], want, "model {i}");
    }
}

#[test]
fn parse_claude_model_doc_ignores_other_rows() {
    let text = "| Amazon Bedrock | Opus 5.5 | Sonnet 4.5 | Haiku 4.5 |\n";
    let models = parse_claude_model_doc(text);
    assert!(models.is_empty(), "expected no models, got {models:?}");
}

#[test]
fn parse_claude_model_doc_strips_markup_and_names_fable() {
    let text = "| **Anthropic API** | [Opus 5.6](https://x) | `Sonnet 5.6` | Haiku 5.6 |\nresolves to Fable 6 everywhere else.";
    let models = parse_claude_model_doc(text);
    let labels: Vec<&str> = models.iter().map(|m| m.label.as_str()).collect();
    assert_eq!(labels, ["Fable 6", "Opus 5.6", "Sonnet 5.6", "Haiku 5.6"]);
}

#[test]
fn installed_agents_limits_to_cli_on_path() {
    let host = FakeHost::new().installed(&["claude", "cursor-agent"]);
    assert_eq!(
        installed_agents_with(&host),
        vec!["cursor".to_string(), "claude".to_string()]
    );
}

#[test]
fn the_catalog_has_one_default_per_tool() {
    let catalog = model_catalog();
    assert_eq!(
        catalog.keys().map(String::as_str).collect::<Vec<_>>(),
        ["claude", "codex", "cursor", "gemini"]
    );
    for tool in AiTool::ALL {
        let models = &catalog[tool.id()];
        assert_eq!(models.iter().filter(|m| m.default).count(), 1, "{tool}");
    }
}

#[test]
fn resolve_model_picks_the_default_or_checks_the_id() {
    let catalog = model_catalog();
    assert_eq!(resolve_model(&catalog, AiTool::Claude, "").unwrap(), "sonnet");
    assert_eq!(resolve_model(&catalog, AiTool::Claude, " opus ").unwrap(), "opus");
    assert_eq!(
        resolve_model(&catalog, AiTool::Claude, "gpt").unwrap_err().to_string(),
        "model is not supported"
    );
    let no_default = catalog_of(
        AiTool::Codex,
        vec![model("a", "A", false, ""), model("b", "B", false, "")],
    );
    assert_eq!(resolve_model(&no_default, AiTool::Codex, "").unwrap(), "a");
    let empty = catalog_of(AiTool::Codex, vec![]);
    assert!(resolve_model(&empty, AiTool::Codex, "").is_err());
    assert!(resolve_model(&empty, AiTool::Gemini, "x").is_err());
}

#[test]
fn live_models_of_an_uninstalled_cli_are_empty() {
    let host = FakeHost::new().installed(&[]);
    for tool in [AiTool::Codex, AiTool::Gemini] {
        assert!(live_models_with(&host, &Ctx::background(), tool).is_empty(), "{tool}");
    }
}
