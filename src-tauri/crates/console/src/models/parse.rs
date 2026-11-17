//! Reading the model lists the CLIs and the Claude Code docs print.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use super::{model, Model};

pub(super) const CURSOR_MODELS_GROUP: &str = "Cursor Models";
pub(super) const OTHER_MODELS_GROUP: &str = "Other Models";

static CLAUDE_FAMILY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(Opus|Sonnet|Haiku) [0-9]+(?:\.[0-9]+)?$").unwrap());
static CLAUDE_FABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"to (Fable [0-9]+(?:\.[0-9]+)?) everywhere else").unwrap());
static CLAUDE_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());

/// Reads `cursor-agent models` text.
pub fn parse_cursor_models(text: &str) -> Vec<Model> {
    let mut models: Vec<Model> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut default_id = String::new();
    for line in text.split('\n') {
        let line = line.trim();
        let Some((id, label)) = line.split_once(" - ") else {
            continue;
        };
        let id = id.trim();
        let mut label = label.trim().to_string();
        if id.is_empty() || id.contains([' ', '\t']) || seen.contains(id) {
            continue;
        }
        if label.to_lowercase().contains("default") {
            default_id = id.to_string();
        }
        if let Some((cut, _)) = label.split_once('(') {
            label = cut.trim().to_string();
        }
        if label.is_empty() {
            label = id.to_string();
        }
        seen.insert(id.to_string());
        models.push(Model {
            id: id.to_string(),
            label,
            default: false,
            group: cursor_model_group(id).to_string(),
        });
    }
    if models.is_empty() {
        return Vec::new();
    }
    if default_id.is_empty() {
        default_id = models[0].id.clone();
    }
    for m in models.iter_mut() {
        m.default = m.id == default_id;
    }
    models
}

/// Matches the Cursor picker: Auto, Composer, and Grok
/// are Cursor Models. Named provider models are Other Models.
/// Grok 4.5 and 4.6 use a cursor-grok- id. Grok 4.7 uses grok-4.7-.
fn cursor_model_group(id: &str) -> &'static str {
    let id = id.trim().to_lowercase();
    if id == "auto" || id.starts_with("composer-") || id.starts_with("cursor-") || id.starts_with("grok-") {
        return CURSOR_MODELS_GROUP;
    }
    OTHER_MODELS_GROUP
}

/// Reads the Anthropic API alias row and the Fable resolution
/// from the Claude Code model config markdown.
pub fn parse_claude_model_doc(text: &str) -> Vec<Model> {
    let Some([opus, sonnet, haiku]) = text.split('\n').find_map(alias_row) else {
        return Vec::new();
    };
    let fable = CLAUDE_FABLE
        .captures(text)
        .map(|caps| caps[1].to_string())
        .unwrap_or_else(|| "Fable 5.2".to_string());
    vec![
        model("fable", &fable, false, ""),
        model("opus", &opus, false, ""),
        model("sonnet", &sonnet, true, ""),
        model("haiku", &haiku, false, ""),
    ]
}

/// The Opus, Sonnet, and Haiku cells of an "Anthropic API" table row.
fn alias_row(line: &str) -> Option<[String; 3]> {
    if !line.contains("Anthropic API") || !line.contains('|') {
        return None;
    }
    let cells = table_cells(line);
    if cells.len() < 4 || !cells[0].contains("Anthropic API") {
        return None;
    }
    if !cells[1..4].iter().all(|cell| CLAUDE_FAMILY.is_match(cell)) {
        return None;
    }
    Some([cells[1].clone(), cells[2].clone(), cells[3].clone()])
}

fn table_cells(line: &str) -> Vec<String> {
    line.split('|')
        .map(clean_cell)
        .filter(|cell| !cell.is_empty())
        .collect()
}

fn clean_cell(value: &str) -> String {
    let value = value.trim().replace("**", "").replace('`', "");
    CLAUDE_LINK.replace_all(&value, "${1}").trim().to_string()
}
