//! Turns an agent's rendered terminal screen into readable text: thinking, replies, and a pending question.

use regex::Regex;
use std::sync::LazyLock;

/// One question in a questions block. Empty `options` means the user types an answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Question {
    pub prompt: String,
    pub detail: String,
    pub options: Vec<String>,
    /// An answer the agent suggests. Empty when it has none.
    pub suggestion: String,
}

/// Ask is a question an agent left for the user.
/// `questions` is the questions block. `prompt` and `options` are the single question
/// recognized when the agent did not use that block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    pub prompt: String,
    pub options: Vec<String>,
    pub questions: Vec<Question>,
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
    if let Some((questions, range)) = questions_block(screen) {
        let mut cleaned = String::new();
        cleaned.push_str(&screen[..range.start]);
        cleaned.push_str(&screen[range.end..]);
        let (text, _) = transcript_body(&cleaned);
        return (
            text,
            Ask {
                questions,
                ..Ask::default()
            },
        );
    }
    transcript_body(screen)
}

/// The transcript without looking for a questions block.
fn transcript_body(screen: &str) -> (String, Ask) {
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
            ..Ask::default()
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

const QUESTIONS_START: &str = "===== QUESTIONS =====";
const QUESTIONS_END: &str = "===== END QUESTIONS =====";

/// The last finished questions block, when nothing the agent said comes after it.
fn questions_block(screen: &str) -> Option<(Vec<Question>, std::ops::Range<usize>)> {
    let start = screen.rfind(QUESTIONS_START)?;
    let body_at = start + QUESTIONS_START.len();
    let end_rel = screen[body_at..].find(QUESTIONS_END)?;
    let body_end = body_at + end_rel;
    let end = body_end + QUESTIONS_END.len();
    let questions = parse_questions(&screen[body_at..body_end]);
    if questions.is_empty() {
        return None;
    }
    let trailing = screen[end..].lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !chrome_line(line)
    });
    if trailing {
        return None;
    }
    Some((questions, start..end))
}

/// Questions inside one block. `A: choice` takes the following `-` lines; anything else is typed.
fn parse_questions(body: &str) -> Vec<Question> {
    let mut questions = Vec::new();
    let mut current: Option<Question> = None;
    let mut choosing = false;
    let mut suggesting = false;
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("Q:") {
            push_question(&mut questions, current.take());
            choosing = false;
            suggesting = false;
            let prompt = name.trim();
            if !prompt.is_empty() {
                current = Some(Question {
                    prompt: prompt.to_string(),
                    ..Question::default()
                });
            }
            continue;
        }
        let Some(question) = current.as_mut() else {
            continue;
        };
        if let Some(kind) = line.strip_prefix("A:") {
            suggesting = false;
            choosing = matches!(kind.trim().to_ascii_lowercase().as_str(), "choice" | "choose");
            if !choosing {
                question.options.clear();
            }
            continue;
        }
        if let Some(text) = line.strip_prefix("S:") {
            suggesting = true;
            append_suggestion(question, text.trim());
            continue;
        }
        if choosing {
            if let Some(option) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
                suggesting = false;
                let option = option.trim();
                if !option.is_empty() {
                    question.options.push(option.to_string());
                }
                continue;
            }
        }
        if suggesting {
            append_suggestion(question, line);
            continue;
        }
        if !question.detail.is_empty() {
            question.detail.push(' ');
        }
        question.detail.push_str(line);
    }
    push_question(&mut questions, current);
    questions
}

fn append_suggestion(question: &mut Question, text: &str) {
    if text.is_empty() {
        return;
    }
    if !question.suggestion.is_empty() {
        question.suggestion.push(' ');
    }
    question.suggestion.push_str(text);
}

fn push_question(questions: &mut Vec<Question>, current: Option<Question>) {
    if let Some(question) = current {
        if !question.prompt.trim().is_empty() {
            questions.push(question);
        }
    }
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

    #[test]
    fn transcript_reads_a_questions_block() {
        let screen = [
            "Thinking",
            "Two things are missing.",
            "===== QUESTIONS =====",
            "Q: Ticket",
            "The Jira key.",
            "A: text",
            "S: PROJ-12",
            "",
            "Q: Scope",
            "What should this cover?",
            "A: choice",
            "- Website only",
            "- Website and mobile",
            "S: Website only",
            "===== END QUESTIONS =====",
            "Auto",
        ]
        .join("\n");
        let (text, ask) = transcript(&screen);
        assert_eq!(text, "Thinking\nTwo things are missing.", "text = {text:?}");
        assert_eq!(ask.prompt, "");
        assert_eq!(
            ask.questions,
            vec![
                Question {
                    prompt: "Ticket".into(),
                    detail: "The Jira key.".into(),
                    options: Vec::new(),
                    suggestion: "PROJ-12".into(),
                },
                Question {
                    prompt: "Scope".into(),
                    detail: "What should this cover?".into(),
                    options: vec!["Website only".into(), "Website and mobile".into()],
                    suggestion: "Website only".into(),
                },
            ]
        );
    }

    #[test]
    fn an_unfinished_questions_block_stays_in_the_transcript() {
        let (text, ask) = transcript("Working.\n===== QUESTIONS =====\nQ: Ticket\nThe Jira key.\nA: text\n");
        assert!(ask.questions.is_empty(), "ask = {ask:?}");
        assert!(text.contains("Q: Ticket"), "text = {text:?}");
    }

    #[test]
    fn the_launch_instruction_is_not_a_question() {
        let screen = "The block begins with this line:\n===== QUESTIONS =====\nThe block ends with this line:\n===== END QUESTIONS =====\n";
        let (text, ask) = transcript(screen);
        assert!(ask.questions.is_empty(), "ask = {ask:?}");
        assert!(text.contains("The block begins"), "text = {text:?}");
    }
}
