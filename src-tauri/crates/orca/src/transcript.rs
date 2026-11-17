//! Turns an agent's rendered terminal screen into readable text: thinking, replies, and a pending question.

use regex::Regex;
use std::sync::LazyLock;

/// Ask is a question an agent left for the user.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    pub prompt: String,
    pub options: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Answer,
    Thinking,
}

struct Spoken {
    kind: LineKind,
    text: String,
}

static OPTION_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[0-9]+[.)]|[-*•]|[❯›])[\t\n\x0C\r ]+(.+)$").unwrap());
static VERSION_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^v[0-9]{4}\.").unwrap());
static SHELL_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[^@\t\n\x0C\r ]+@[^\t\n\x0C\r ]+").unwrap());
static USER_ASK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(should i|do you|would you|can i|please|which|what should|could you|confirm)(?-u:\b)").unwrap()
});

/// Transcript keeps an agent's thinking and replies, and a question when the agent is waiting.
pub fn transcript(screen: &str) -> (String, Ask) {
    let mut lines = spoken_lines(screen);
    let ask = match question_at_end(&lines) {
        Some((ask, at)) => {
            lines.truncate(at);
            ask
        }
        None => Ask::default(),
    };
    let join = |kind: LineKind| {
        let texts: Vec<&str> = lines
            .iter()
            .filter(|line| line.kind == kind)
            .map(|line| line.text.as_str())
            .collect();
        texts.join("\n")
    };
    let mut out = String::new();
    let thinking = join(LineKind::Thinking);
    if !thinking.is_empty() {
        out.push_str("Thinking\n");
        out.push_str(&thinking);
    }
    let answer = join(LineKind::Answer);
    if !answer.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&answer);
    }
    (out.trim().to_string(), ask)
}

/// The screen's content lines, each marked as thinking or answer. A blank line ends a thinking block.
fn spoken_lines(screen: &str) -> Vec<Spoken> {
    let mut lines: Vec<Spoken> = Vec::new();
    let mut thinking = false;
    for line in screen.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            thinking = false;
            continue;
        }
        if chrome_line(line) {
            continue;
        }
        if is_thinking_header(line) {
            thinking = true;
            continue;
        }
        let kind = if thinking { LineKind::Thinking } else { LineKind::Answer };
        lines.push(Spoken {
            kind,
            text: line.to_string(),
        });
        if kind == LineKind::Answer {
            thinking = false;
        }
    }
    lines
}

/// A question that ends the screen: the question line and the choices after it. Returns the question and
/// the index of its line; the lines from there on are not part of the transcript.
fn question_at_end(lines: &[Spoken]) -> Option<(Ask, usize)> {
    let mut options: Vec<String> = Vec::new();
    let mut start = lines.len();
    while start > 0 {
        let Some(option) = option_line(&lines[start - 1].text) else {
            break;
        };
        options.push(option);
        start -= 1;
    }
    options.reverse();
    let at = start.checked_sub(1)?;
    let question = &lines[at].text;
    if !question.ends_with('?') || (options.is_empty() && !user_question(question)) {
        return None;
    }
    Some((
        Ask {
            prompt: question.clone(),
            options,
        },
        at,
    ))
}

/// Terminal decoration that is not part of what the agent said.
fn chrome_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.contains("pasted text")
        || lower.contains("run everything")
        || lower.contains("npmrc")
        || lower.contains("nvm ")
        || lower.starts_with("tip:")
        || lower == "cursor agent"
        || lower == "auto"
        || line.starts_with("~/")
        || (line.contains('@') && line.contains("cursor-agent"))
    {
        return true;
    }
    if VERSION_LINE.is_match(line) || SHELL_LINE.is_match(line) {
        return true;
    }
    !line.chars().any(|c| c.is_alphabetic() || c.is_numeric())
}

fn is_thinking_header(line: &str) -> bool {
    matches!(
        line.trim_matches(&[':', ' '][..]).to_lowercase().as_str(),
        "thinking" | "thought" | "thoughts" | "reasoning"
    )
}

/// The text of a choice line (`1. text`, `- text`, `❯ text`); a line that is itself a question is not a choice.
fn option_line(line: &str) -> Option<String> {
    let caps = OPTION_LINE.captures(line)?;
    let text = caps.get(1)?.as_str().trim();
    if text.is_empty() || text.ends_with('?') {
        return None;
    }
    Some(text.to_string())
}

fn user_question(line: &str) -> bool {
    USER_ASK.is_match(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_keeps_thinking_and_answer() {
        let screen = [
            "Cursor Agent",
            "v2026.11.13",
            "Tip: Use /plan to plan ahead.",
            "[Pasted text #1 +647 lines]",
            "Thinking",
            "The invoice page needs a handler.",
            "",
            "I added the invoice route and its test.",
            "Auto",
            "~/Documents/Goldeneye/ellum/docs · main",
            "Run Everything",
        ]
        .join("\n");
        let (text, ask) = transcript(&screen);
        assert_eq!(ask.prompt, "", "ask = {ask:?}");
        let want = "Thinking\nThe invoice page needs a handler.\n\nI added the invoice route and its test.";
        assert_eq!(text, want, "text = {text:?}");
    }

    #[test]
    fn transcript_question_with_choices() {
        let screen = [
            "Thinking",
            "Two shapes fit this task.",
            "Which one should I write?",
            "1. Short brief",
            "2. Full specification",
        ]
        .join("\n");
        let (text, ask) = transcript(&screen);
        assert!(
            ask.prompt == "Which one should I write?" && ask.options.len() == 2 && ask.options[0] == "Short brief",
            "ask = {ask:?}"
        );
        assert_eq!(text, "Thinking\nTwo shapes fit this task.", "text = {text:?}");
    }

    #[test]
    fn a_question_without_choices_needs_a_user_ask_opening() {
        let (text, ask) = transcript("Done.\nDo you want the tests too?");
        assert_eq!(ask.prompt, "Do you want the tests too?");
        assert!(ask.options.is_empty());
        assert_eq!(text, "Done.");
        let (text, ask) = transcript("Is this fine?");
        assert_eq!(ask, Ask::default());
        assert_eq!(text, "Is this fine?");
        let (text, ask) = transcript("1. only\n2. choices");
        assert_eq!(ask, Ask::default());
        assert_eq!(text, "1. only\n2. choices");
    }
}
