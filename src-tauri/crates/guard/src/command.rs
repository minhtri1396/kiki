//! The shell command that runs this app's guardrail, with the executable path quoted.

/// The command that runs this app's guardrail, without the AI tool name.
pub fn hook_command(exe: &str) -> String {
    let mut exe = exe.trim();
    if exe.is_empty() {
        exe = "kiki";
    }
    format!("{} guardrail", quote(exe))
}

/// Double-quoted text with backslash escapes. Hook files on users' disks hold this exact form, so it is spelled out
/// instead of relying on `{:?}`, whose escapes differ for non-ASCII and control characters.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            c if is_print(c) => out.push(c),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push('"');
    out
}

/// Graphic characters plus the ASCII space; control, format, and private-use characters are escaped.
fn is_print(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_control() || c.is_whitespace() {
        return false;
    }
    !matches!(c as u32,
        0xad | 0x600..=0x605 | 0x61c | 0x6dd | 0x70f | 0x180e | 0x200b..=0x200f | 0x202a..=0x202e
        | 0x2060..=0x206f | 0xfeff | 0xfff9..=0xfffb | 0xe000..=0xf8ff | 0xf0000..=0x10ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_the_executable_and_defaults_to_kiki() {
        assert_eq!(hook_command("/Applications/KIKI"), "\"/Applications/KIKI\" guardrail");
        assert_eq!(hook_command("  "), "\"kiki\" guardrail");
        assert_eq!(hook_command("/a b/\"x\"\\y"), "\"/a b/\\\"x\\\"\\\\y\" guardrail");
    }

    #[test]
    fn escapes_control_and_invisible_characters() {
        assert_eq!(quote("a\tb\u{1}c\u{7f}"), "\"a\\tb\\x01c\\x7f\"");
        assert_eq!(quote("\u{200b}\u{10ffff}"), "\"\\u200b\\U0010ffff\"");
        assert_eq!(quote("héllo"), "\"héllo\"");
    }
}
