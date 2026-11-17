//! Splits a shell command line into segments and words. This is a lexer for guardrail decisions, not a shell:
//! it knows quotes, `;`, newlines, `|`, `||` and `&&`, and nothing else.

/// Splits a command line at `;`, newlines, `|`, `||` and `&&` outside quotes. Segments are trimmed and not empty.
pub(super) fn split_command(command: &str) -> Vec<String> {
    let bytes = command.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut b: Vec<u8> = Vec::new();
    let mut quote: u8 = 0;
    // Only ASCII delimiters split the text, so every segment stays valid UTF-8.
    let flush = |b: &mut Vec<u8>, out: &mut Vec<String>| {
        let text = String::from_utf8_lossy(b).trim().to_string();
        b.clear();
        if !text.is_empty() {
            out.push(text);
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        let r = bytes[i];
        if quote != 0 {
            b.push(r);
            if r == quote {
                quote = 0;
            }
            i += 1;
            continue;
        }
        if r == b'\'' || r == b'"' {
            quote = r;
            b.push(r);
            i += 1;
            continue;
        }
        if r == b'\n' || r == b';' {
            flush(&mut b, &mut out);
            i += 1;
            continue;
        }
        if r == b'|' || (r == b'&' && i + 1 < bytes.len() && bytes[i + 1] == b'&') {
            flush(&mut b, &mut out);
            if r == b'&' || (i + 1 < bytes.len() && bytes[i + 1] == b'|') {
                i += 1;
            }
            i += 1;
            continue;
        }
        b.push(r);
        i += 1;
    }
    flush(&mut b, &mut out);
    out
}

/// Splits a segment into words at spaces and tabs; quotes group words and are dropped.
pub(super) fn tokenize(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut b = String::new();
    let mut quote: Option<char> = None;
    for r in command.chars() {
        if let Some(q) = quote {
            if r == q {
                quote = None;
                continue;
            }
            b.push(r);
        } else if r == '\'' || r == '"' {
            quote = Some(r);
        } else if r == ' ' || r == '\t' || r == '\n' || r == '\r' {
            if !b.is_empty() {
                out.push(std::mem::take(&mut b));
            }
        } else {
            b.push(r);
        }
    }
    if !b.is_empty() {
        out.push(b);
    }
    out
}

/// The program a segment runs (lower case, without its folder) and the words after it.
/// Variable assignments, wrappers like `sudo`, and option words before the program are skipped.
pub(super) fn segment_command(tokens: &[String]) -> (String, &[String]) {
    for (i, tok) in tokens.iter().enumerate() {
        if is_assignment(tok) || is_wrapper(&base_name(tok)) {
            continue;
        }
        if tok.starts_with('-') {
            continue;
        }
        return (base_name(tok), &tokens[i + 1..]);
    }
    (String::new(), &[])
}

fn is_assignment(tok: &str) -> bool {
    let eq = match tok.find('=') {
        Some(eq) if eq > 0 => eq,
        _ => return false,
    };
    if tok.starts_with('-') {
        return false;
    }
    !tok[..eq].contains(['/', '\\'])
}

fn is_wrapper(name: &str) -> bool {
    matches!(name, "sudo" | "command" | "env" | "time" | "nice" | "nohup" | "exec")
}

fn base_name(tok: &str) -> String {
    let tok = tok.trim().trim_matches(|c| c == '"' || c == '\'');
    if tok.is_empty() {
        return String::new();
    }
    let tok = match tok.rfind(['/', '\\']) {
        Some(i) => &tok[i + 1..],
        None => tok,
    };
    tok.to_lowercase()
}
