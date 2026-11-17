//! Text helpers: command output cleaning, log chunk decoding, and command display.

use regex::Regex;
use std::sync::LazyLock;

static ANSI_ESCAPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").unwrap());

/// Strips terminal colour codes and surrounding whitespace.
pub(crate) fn clean_output(text: &str) -> String {
    ANSI_ESCAPE.replace_all(text, "").trim().to_string()
}

/// The first non-empty line of the cleaned output.
pub(crate) fn first_line(text: &str) -> String {
    clean_output(text)
        .split('\n')
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Cuts `summary` at a byte offset; a split character becomes U+FFFD.
pub(crate) fn truncate_bytes(summary: String, n: usize) -> String {
    if summary.len() > n {
        String::from_utf8_lossy(&summary.as_bytes()[..n]).into_owned()
    } else {
        summary
    }
}

/// Appends raw bytes to `pending` and returns the longest text that can be decoded now.
/// An incomplete trailing UTF-8 sequence stays in `pending` for the next chunk; invalid
/// bytes become U+FFFD.
pub(crate) fn decode_chunk(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    pending.extend_from_slice(chunk);
    let mut out = String::new();
    loop {
        let err = match std::str::from_utf8(pending) {
            Ok(s) => {
                out.push_str(s);
                pending.clear();
                return out;
            }
            Err(e) => e,
        };
        let valid = err.valid_up_to();
        out.push_str(std::str::from_utf8(&pending[..valid]).unwrap_or(""));
        match err.error_len() {
            Some(bad) => {
                out.push('\u{FFFD}');
                pending.drain(..valid + bad);
            }
            None => {
                pending.drain(..valid);
                return out;
            }
        }
    }
}

/// The log text that starts a job: the trimmed note and a newline, or nothing.
pub(crate) fn prefix_log(prefix: &str) -> String {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return String::new();
    }
    format!("{prefix}\n")
}

/// FormatArgs joins a command for display. Arguments with blanks are quoted.
pub fn format_args(args: &[String]) -> String {
    let quoted: Vec<String> = args
        .iter()
        .map(|arg| {
            if arg.contains([' ', '\t']) {
                quote_arg(arg)
            } else {
                arg.clone()
            }
        })
        .collect();
    quoted.join(" ")
}

/// Double-quotes `s` with backslash escapes for quotes, control characters and unprintable characters.
fn quote_arg(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x7f => out.push(c),
            c if c.is_control() || c.is_whitespace() => {
                if (c as u32) < 0x10000 {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                } else {
                    out.push_str(&format!("\\U{:08x}", c as u32));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_chunk_keeps_incomplete_sequences() {
        let mut pending = Vec::new();
        let euro = "€".as_bytes();
        assert_eq!(decode_chunk(&mut pending, &euro[..2]), "");
        assert_eq!(decode_chunk(&mut pending, &euro[2..]), "€");
        assert!(pending.is_empty());
        assert_eq!(decode_chunk(&mut pending, b"a\xffb"), "a\u{FFFD}b");
    }

    #[test]
    fn format_args_quotes_blanks() {
        let args = [
            "codegraph".to_string(),
            "init".to_string(),
            "/tmp/my dir".to_string(),
            "t\"q ".to_string(),
        ];
        assert_eq!(format_args(&args), "codegraph init \"/tmp/my dir\" \"t\\\"q \"");
        assert_eq!(quote_arg("a\tb\u{1}"), "\"a\\tb\\x01\"");
    }

    #[test]
    fn output_helpers() {
        assert_eq!(clean_output("\x1b[31m red \x1b[0m\n"), "red");
        assert_eq!(first_line("\n  \n x \ny"), "x");
        assert_eq!(truncate_bytes("héllo".to_string(), 2), "h\u{FFFD}");
        assert_eq!(prefix_log("  note "), "note\n");
        assert_eq!(prefix_log(" "), "");
    }
}
