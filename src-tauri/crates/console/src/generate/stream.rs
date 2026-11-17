//! Reading a running CLI: stdout is decoded into events, stderr is shown as plain log lines.

use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use crate::host::CommandRun;
use crate::text::one_line;

use super::events::parse_event;
use super::{GenerateEvent, GenerateSink};

/// The longest line the readers accept; a longer line ends the read.
const MAX_LINE_BYTES: usize = 1024 * 1024;

/// What the CLI has said so far.
#[derive(Default)]
pub(super) struct StreamState {
    thinking: String,
    pub(super) assistant: String,
    pub(super) plain: String,
    pub(super) final_text: String,
    pub(super) failed: bool,
    pub(super) fail_message: String,
}

struct LogPiece {
    text: String,
    channel: &'static str,
}

fn piece(text: &str, channel: &'static str) -> LogPiece {
    LogPiece {
        text: text.to_string(),
        channel,
    }
}

/// Logs the new part of a streamed field. A later message that repeats the
/// text already received is ignored.
fn grow(have: &mut String, next: &str, channel: &'static str) -> Vec<LogPiece> {
    if next.is_empty() || next == have.as_str() {
        return Vec::new();
    }
    let extra: String;
    if !have.is_empty() && next.starts_with(have.as_str()) {
        extra = next[have.len()..].to_string();
        *have = next.to_string();
    } else if !have.is_empty() && have.starts_with(next) {
        return Vec::new();
    } else {
        extra = next.to_string();
        have.push_str(next);
    }
    if extra.is_empty() {
        return Vec::new();
    }
    vec![piece(&extra, channel)]
}

impl StreamState {
    /// Reads one stdout line and returns the log pieces to show.
    fn consume(&mut self, line: &str) -> Vec<LogPiece> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let event = match parse_event(trimmed) {
            Some(event) if !event.kind.is_empty() => event,
            _ => {
                self.plain.push_str(line);
                self.plain.push('\n');
                return vec![piece(&format!("{line}\n"), "text")];
            }
        };
        match event.kind.as_str() {
            "thinking" => {
                if event.subtype == "completed" {
                    return vec![piece("\n", "think")];
                }
                let body = if event.text.is_empty() {
                    &event.thinking
                } else {
                    &event.text
                };
                grow(&mut self.thinking, body, "think")
            }
            "stream_event" | "content_block_delta" => {
                let delta = if event.kind == "content_block_delta" {
                    &event.delta
                } else {
                    &event.event_delta
                };
                self.delta_pieces(&delta.kind, &delta.text, &delta.thinking)
            }
            "assistant" => self.assistant_pieces(&event.content),
            "item.completed" => {
                if event.item_kind == "thinking" || event.item_kind == "reasoning" {
                    grow(&mut self.thinking, &event.item_text, "think")
                } else {
                    grow(&mut self.assistant, &event.item_text, "text")
                }
            }
            "result" => {
                self.final_text = event.result.clone();
                if event.is_error || event.subtype == "error" {
                    self.failed = true;
                    self.fail_message = one_line(&event.result);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A whole assistant message: its thinking blocks and its answer blocks.
    fn assistant_pieces(&mut self, content: &[super::events::Block]) -> Vec<LogPiece> {
        let mut thought = String::new();
        let mut answer = String::new();
        for block in content {
            match block.kind.as_str() {
                "thinking" | "reasoning" => {
                    if !block.thinking.is_empty() {
                        thought.push_str(&block.thinking);
                    } else {
                        thought.push_str(&block.text);
                    }
                }
                "text" | "" => answer.push_str(&block.text),
                _ => {}
            }
        }
        let mut pieces = grow(&mut self.thinking, &thought, "think");
        pieces.extend(grow(&mut self.assistant, &answer, "text"));
        pieces
    }

    fn delta_pieces(&mut self, kind: &str, text: &str, thinking: &str) -> Vec<LogPiece> {
        let mut pieces = Vec::new();
        if !thinking.is_empty() {
            pieces.extend(grow(&mut self.thinking, thinking, "think"));
        } else if kind == "thinking_delta" || kind == "reasoning_delta" {
            pieces.extend(grow(&mut self.thinking, text, "think"));
        }
        if !text.is_empty() && thinking.is_empty() && kind != "thinking_delta" && kind != "reasoning_delta" {
            pieces.extend(grow(&mut self.assistant, text, "text"));
        }
        pieces
    }

    /// The text the model produced: the final result, else the streamed answer, else any plain output.
    pub(super) fn best_text(&self) -> &str {
        [&self.final_text, &self.assistant, &self.plain]
            .into_iter()
            .map(String::as_str)
            .find(|text| !text.is_empty())
            .unwrap_or("")
    }
}

/// Reads one line: the trailing `\n` (and a `\r` before it) are dropped. `None` ends the scan,
/// on end of input, a read error, or a line over 1 MiB.
fn scan_line(reader: &mut BufReader<Box<dyn Read + Send>>) -> Option<String> {
    let mut buf = Vec::new();
    match reader.read_until(b'\n', &mut buf) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }
    if buf.len() > MAX_LINE_BYTES {
        return None;
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn log_event(channel: &str, text: String) -> GenerateEvent {
    GenerateEvent {
        r#type: "log".into(),
        channel: channel.into(),
        text,
        ..Default::default()
    }
}

fn read_generate_lines(
    r: Option<Box<dyn Read + Send>>,
    state: &mut StreamState,
    stop: &AtomicBool,
    emit: &dyn Fn(GenerateEvent),
) {
    let Some(r) = r else { return };
    let mut reader = BufReader::new(r);
    while let Some(line) = scan_line(&mut reader) {
        for p in state.consume(&line) {
            if p.text.is_empty() {
                continue;
            }
            if stop.load(Ordering::SeqCst) {
                return;
            }
            emit(log_event(p.channel, p.text));
        }
        if stop.load(Ordering::SeqCst) {
            return;
        }
    }
}

fn read_plain_log(r: Option<Box<dyn Read + Send>>, stop: &AtomicBool, emit: &dyn Fn(GenerateEvent)) {
    let Some(r) = r else { return };
    let mut reader = BufReader::new(r);
    while let Some(line) = scan_line(&mut reader) {
        if line.trim().is_empty() {
            continue;
        }
        if stop.load(Ordering::SeqCst) {
            return;
        }
        emit(log_event("text", format!("{line}\n")));
    }
}

/// What reading a CLI to the end found.
pub(super) struct Outcome {
    pub(super) state: StreamState,
    /// How the process ended, when it ended badly.
    pub(super) wait_err: Option<anyhow::Error>,
}

/// Reads both streams until they close, hands every log event to `sink` one at a time, then waits for the process.
///
/// A reader whose event the sink refused stops reading.
pub(super) fn read_output(run: CommandRun, sink: &mut dyn GenerateSink) -> Outcome {
    let CommandRun { stdout, stderr, wait } = run;
    let (tx, rx) = mpsc::channel::<(usize, GenerateEvent)>();
    let stop = [AtomicBool::new(false), AtomicBool::new(false)];
    let stop_ref = &stop;
    let tx_out = tx.clone();
    let tx_err = tx;
    let state = std::thread::scope(|s| {
        let out_handle = s.spawn(move || {
            let mut state = StreamState::default();
            read_generate_lines(stdout, &mut state, &stop_ref[0], &|ev| {
                let _ = tx_out.send((0, ev));
            });
            state
        });
        s.spawn(move || {
            read_plain_log(stderr, &stop_ref[1], &|ev| {
                let _ = tx_err.send((1, ev));
            });
        });
        for (id, ev) in rx {
            if stop[id].load(Ordering::SeqCst) {
                continue;
            }
            if sink.log(ev).is_err() {
                stop[id].store(true, Ordering::SeqCst);
            }
        }
        out_handle.join().unwrap_or_default()
    });
    Outcome {
        state,
        wait_err: wait().err(),
    }
}

/// Strips one surrounding Markdown code fence from the model's answer.
pub(super) fn clean_generated(text: &str) -> String {
    let text = text.trim();
    let Some(rest) = text.strip_prefix("```") else {
        return text.to_string();
    };
    let Some(newline) = rest.find('\n') else {
        return text.to_string();
    };
    let body = rest[newline + 1..].trim();
    let Some(body) = body.strip_suffix("```") else {
        return text.to_string();
    };
    let body = body.trim();
    if body.is_empty() {
        return text.to_string();
    }
    body.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logged(state: &mut StreamState, line: &str) -> Vec<String> {
        state
            .consume(line)
            .into_iter()
            .map(|p| format!("{}:{}", p.channel, p.text))
            .collect()
    }

    #[test]
    fn plain_lines_become_text_and_the_fallback_answer() {
        let mut state = StreamState::default();
        assert_eq!(logged(&mut state, "hello"), ["text:hello\n"]);
        assert!(logged(&mut state, "   ").is_empty());
        assert_eq!(state.best_text(), "hello\n");
    }

    #[test]
    fn best_text_prefers_result_then_assistant_then_plain() {
        let mut state = StreamState {
            plain: "p".into(),
            ..Default::default()
        };
        assert_eq!(state.best_text(), "p");
        state.assistant = "a".into();
        assert_eq!(state.best_text(), "a");
        state.final_text = "f".into();
        assert_eq!(state.best_text(), "f");
    }

    #[test]
    fn a_repeated_message_is_logged_once() {
        let mut state = StreamState::default();
        let line = |text: &str| {
            format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{text}"}}]}}}}"#)
        };
        assert_eq!(logged(&mut state, &line("Clear")), ["text:Clear"]);
        assert_eq!(logged(&mut state, &line("Clearer")), ["text:er"]);
        assert!(logged(&mut state, &line("Clearer")).is_empty());
        assert!(logged(&mut state, &line("Clear")).is_empty());
    }

    #[test]
    fn an_error_result_fails_the_run() {
        let mut state = StreamState::default();
        state.consume(r#"{"type":"result","subtype":"error","result":"rate\nlimited"}"#);
        assert!(state.failed);
        assert_eq!(state.fail_message, "rate limited");
    }

    #[test]
    fn code_fences_are_stripped_once() {
        assert_eq!(clean_generated("  ```text\nHello\n```  "), "Hello");
        assert_eq!(clean_generated("```\n```"), "```\n```");
        assert_eq!(clean_generated("no fence"), "no fence");
    }

    #[test]
    fn scan_line_drops_line_endings_and_huge_lines() {
        let source = |bytes: Vec<u8>| BufReader::new(Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>);
        let mut reader = source(b"a\r\nb\n".to_vec());
        assert_eq!(scan_line(&mut reader).as_deref(), Some("a"));
        assert_eq!(scan_line(&mut reader).as_deref(), Some("b"));
        assert_eq!(scan_line(&mut reader), None);
        assert_eq!(scan_line(&mut source(vec![b'x'; MAX_LINE_BYTES + 1])), None);
    }
}
