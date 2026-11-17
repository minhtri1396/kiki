//! The instruction sent to a CLI when it rewrites a piece of text.

use anyhow::Result;

use crate::error::input_error;
use crate::text::one_line;

const MAX_TEXT_BYTES: usize = 60000;

/// The instruction sent to the CLI. It asks for a rewrite of text.
pub fn rewrite_prompt(kind: &str, subject: &str, text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(input_error("Enter some text first."));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(input_error("Text is too long to generate."));
    }
    let subject = one_line(subject);
    if kind == "branch" {
        return Ok(format!("This task name will become part of a git branch.\n\nRewrite it as a short English name of two to five words. Keep the same meaning. Drop ticket ids and punctuation. Return only the English name.\n\n{text}"));
    }
    let what = describe(kind, &subject)?;
    Ok(format!("{what}\n\nRewrite the text below in English. Another AI model will read it and must do exactly what it describes, so make the text concise, complete, and clear. Keep the same meaning, names, ids, and paths. Return only the rewritten text.\n\n{text}"))
}

/// The first sentence of the instruction: what the text is for.
fn describe(kind: &str, subject: &str) -> Result<String> {
    Ok(match kind {
        "prompt" if subject.is_empty() => "This is an agent prompt.".to_string(),
        "prompt" => format!("This is the prompt for agent {subject}."),
        "repo" => named(subject, "Choose a repository first.", |s| {
            format!("This says what the agent does in the {s} repository.")
        })?,
        "teamwork" => named(subject, "Choose an agent first.", |s| {
            format!("This is the message the agent sends to agent {s} when its condition is met.")
        })?,
        "teamwork_when" => named(subject, "Choose an agent first.", |s| {
            format!("This is the condition under which the agent sends a message to agent {s}. The agent checks it while it works, so state it as something the agent can observe.")
        })?,
        "teamwork_reply" => {
            named(subject, "Choose an agent first.", |s| {
                format!("This is what agent {s} sends back to the agent that gave it work, after it has finished that work.")
            })?
        }
        "mcp" => named(subject, "Choose an MCP server first.", |s| {
            format!("This says what the agent does with the {s} MCP server.")
        })?,
        "tool" => named(subject, "Choose a tool first.", |s| {
            format!("This says what the agent does with the {s} tool.")
        })?,
        "task" => "This is a task description. Keep the rewrite within 4000 characters.".to_string(),
        "task-name" => "This is a task name. Keep the rewrite on one line and within 200 characters.".to_string(),
        _ => {
            return Err(input_error(
                "Choose a prompt, repository note, Agents Coordination note, MCP note, or tool note.",
            ));
        }
    })
}

/// A sentence about `subject`; an empty subject is the `missing` input error.
fn named(subject: &str, missing: &str, sentence: impl FnOnce(&str) -> String) -> Result<String> {
    if subject.is_empty() {
        return Err(input_error(missing));
    }
    Ok(sentence(subject))
}
