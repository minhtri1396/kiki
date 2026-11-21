//! The instruction sent to a CLI when it rewrites a piece of text.

use anyhow::Result;

use crate::error::input_error;
use crate::text::one_line;

const MAX_TEXT_BYTES: usize = 60000;
const MAX_PROMPT_BYTES: usize = 200_000;

/// Facts the rewrite may use besides the text the user is editing.
/// Empty fields are left out of the instruction.
#[derive(Debug, Clone, Copy, Default)]
pub struct RewriteContext<'a> {
    /// The prompt of the agent being edited.
    pub prompt: &'a str,
    /// Name, id, and description of the agent being edited.
    pub agent: &'a str,
    /// Name, id, and description of the agent named in Agents Coordination.
    pub peer: &'a str,
    /// The Send when text of this message.
    pub when: &'a str,
    /// The What to send text of this message.
    pub send: &'a str,
}

/// The instruction sent to the CLI. It asks for a rewrite of text.
pub fn rewrite_prompt(kind: &str, subject: &str, text: &str, context: RewriteContext) -> Result<String> {
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
    let mut prompt = format!("{what}\n\n{}", rewrite_rule(kind));
    append_context(&mut prompt, kind, context);
    prompt.push_str("\n\nText to rewrite:\n");
    prompt.push_str(text);
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(input_error("Text is too long to generate."));
    }
    Ok(prompt)
}

/// How to rewrite this kind of text. The result stays in English.
fn rewrite_rule(kind: &str) -> &'static str {
    match kind {
        "prompt" => "Rewrite the text below in English. Base the rewrite on the user's text. Make it clear and concise. Another AI model will follow it as its instructions and must do exactly what the user described. Keep the same meaning, names, ids, and paths. Do not add work the user did not describe. Return only the rewritten text.",
        "prompt_field" => "Rewrite the text below in English. It is one field of an agent prompt, not the whole prompt. Base the rewrite on the user's text. Make that field clear and concise. Keep the same meaning, names, ids, and paths. Do not add other fields, sections, or work the user did not write. Return only the rewritten field.",
        "prompt_input" | "prompt_output" => "Rewrite every field below in English. Each field begins with a heading line `## ` followed by the field name. Keep every heading, in the same order, and rewrite only the text under that heading. A field the user left empty stays empty. Keep the same meaning, names, ids, and paths. Do not add, drop, or rename a field. Do not add work the user did not write. Return only the headings and the rewritten fields.",
        "teamwork_when" => "Rewrite the text below in English. It is the condition the first agent checks while it works, so state it as something that agent can observe. Base the rewrite on the user's text, and use the agent prompt and the two descriptions so the condition fits that work. Keep the same meaning. Do not add a condition the user did not write. Return only the rewritten condition.",
        "teamwork" => "Rewrite the text below in English. It is the message the first agent sends when the condition is met. Another AI model will read it and must do exactly what it describes. Base the rewrite on the user's text, and use the agent prompt, the send-when condition, and the two descriptions so the message fits that work. Keep the same meaning, names, ids, and paths. Do not add work the user did not describe. Return only the rewritten message.",
        "teamwork_reply" => "Rewrite the text below in English. It is what the other agent reports back after it finishes the work it was sent. Base the rewrite on the user's text, and use the agent prompt, the send-when condition, the message that was sent, and the two descriptions so the report fits that work. Keep the same meaning, names, ids, and paths. Do not add a report the user did not describe. Return only the rewritten report.",
        _ => "Rewrite the text below in English. Another AI model will read it and must do exactly what it describes, so make the text concise, complete, and clear. Keep the same meaning, names, ids, and paths. Return only the rewritten text.",
    }
}

/// The surrounding facts for an Agents Coordination rewrite. Other kinds ignore them.
fn append_context(out: &mut String, kind: &str, context: RewriteContext) {
    if !matches!(kind, "teamwork" | "teamwork_when" | "teamwork_reply") {
        return;
    }
    push_section(out, "Agent prompt", context.prompt);
    push_section(out, "This agent", context.agent);
    push_section(out, "The other agent", context.peer);
    if matches!(kind, "teamwork" | "teamwork_reply") {
        push_section(out, "Send when", context.when);
    }
    if kind == "teamwork_reply" {
        push_section(out, "What to send", context.send);
    }
}

fn push_section(out: &mut String, title: &str, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    out.push_str("\n\n");
    out.push_str(title);
    out.push_str(":\n");
    out.push_str(body);
}

/// The first sentence of the instruction: what the text is for.
fn describe(kind: &str, subject: &str) -> Result<String> {
    Ok(match kind {
        "prompt" if subject.is_empty() => "This is an agent prompt.".to_string(),
        "prompt" => format!("This is the prompt for agent {subject}."),
        "prompt_field" if subject.is_empty() => "This is one field of an agent prompt.".to_string(),
        "prompt_field" => format!("This is one field of the prompt for agent {subject}."),
        "prompt_input" if subject.is_empty() => "These are the fields of one input in an agent prompt.".to_string(),
        "prompt_input" => format!("These are the fields of one input in the prompt for agent {subject}."),
        "prompt_output" if subject.is_empty() => "These are the fields of one output in an agent prompt.".to_string(),
        "prompt_output" => format!("These are the fields of one output in the prompt for agent {subject}."),
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
