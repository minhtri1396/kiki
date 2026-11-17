//! Scalar styles: how a string is written (plain, single quoted, double quoted or literal block).
//!
//! A port of libyaml's `yaml_emitter_analyze_scalar` and `yaml_emitter_select_scalar_style` as yaml.v3
//! uses them, so the files stay identical to the ones yaml.v3 writes.

use crate::resolve::{is_base60_float, is_old_bool, resolve, Resolved};

/// libyaml's `is_printable`, as yaml.v3 ships it: no characters above U+FFFF.
fn is_printable(c: char) -> bool {
    let c = c as u32;
    c == 0x0A
        || (0x20..=0x7E).contains(&c)
        || (0xA0..=0xD7FF).contains(&c)
        || ((0xE000..=0xFFFD).contains(&c) && c != 0xFEFF)
}

fn is_break(c: char) -> bool {
    matches!(c, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

#[derive(PartialEq, Clone, Copy)]
enum Style {
    Plain,
    Single,
    Double,
    Literal,
}

/// The scalar style yaml.v3 picks for a string. `allow_literal` is false where a `|` block cannot be written
/// (a mapping key, or a value that must stay on one line); such text is double quoted instead.
fn string_style(s: &str, allow_literal: bool) -> Style {
    let can_use_plain = resolve(s) == Resolved::Str && !(is_base60_float(s) || is_old_bool(s));
    let mut style = if s.contains('\n') && allow_literal {
        Style::Literal
    } else if !allow_literal && s.chars().any(is_break) {
        Style::Double
    } else if can_use_plain {
        Style::Plain
    } else {
        Style::Double
    };

    // yaml_emitter_analyze_scalar
    let chars: Vec<char> = s.chars().collect();
    let (block_plain_allowed, single_quoted_allowed, block_allowed);
    if chars.is_empty() {
        block_plain_allowed = true;
        single_quoted_allowed = true;
        block_allowed = false;
    } else {
        let mut block_indicators = false;
        let mut line_breaks = false;
        let mut special_characters = false;
        let mut tab_characters = false;
        let (mut leading_space, mut leading_break, mut trailing_space, mut trailing_break) =
            (false, false, false, false);
        let (mut break_space, mut space_break) = (false, false);
        let mut preceded_by_whitespace = true;
        let (mut previous_space, mut previous_break) = (false, false);
        if s.starts_with("---") || s.starts_with("...") {
            block_indicators = true;
        }
        let blank = |c: char| c == ' ' || c == '\t';
        for (i, &c) in chars.iter().enumerate() {
            let followed_by_whitespace = i + 1 >= chars.len() || blank(chars[i + 1]);
            if i == 0 {
                match c {
                    '#' | ',' | '[' | ']' | '{' | '}' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`' => {
                        block_indicators = true;
                    }
                    '?' | ':' | '-' if followed_by_whitespace => block_indicators = true,
                    _ => {}
                }
            } else {
                match c {
                    ':' if followed_by_whitespace => block_indicators = true,
                    '#' if preceded_by_whitespace => block_indicators = true,
                    _ => {}
                }
            }
            if c == '\t' {
                tab_characters = true;
            } else if !is_printable(c) {
                special_characters = true;
            }
            if c == ' ' {
                if i == 0 {
                    leading_space = true;
                }
                if i + 1 == chars.len() {
                    trailing_space = true;
                }
                if previous_break {
                    break_space = true;
                }
                previous_space = true;
                previous_break = false;
            } else if is_break(c) {
                line_breaks = true;
                if i == 0 {
                    leading_break = true;
                }
                if i + 1 == chars.len() {
                    trailing_break = true;
                }
                if previous_space {
                    space_break = true;
                }
                previous_space = false;
                previous_break = true;
            } else {
                previous_space = false;
                previous_break = false;
            }
            preceded_by_whitespace = blank(c) || is_break(c);
        }
        let mut plain = true;
        let mut single = true;
        let mut block = true;
        if leading_space || leading_break || trailing_space || trailing_break {
            plain = false;
        }
        if trailing_space {
            block = false;
        }
        if break_space {
            plain = false;
            single = false;
        }
        if space_break || tab_characters || special_characters {
            plain = false;
            single = false;
        }
        if space_break || special_characters {
            block = false;
        }
        if line_breaks {
            plain = false;
        }
        if block_indicators {
            plain = false;
        }
        block_plain_allowed = plain;
        single_quoted_allowed = single;
        block_allowed = block;
    }

    // yaml_emitter_select_scalar_style
    if style == Style::Plain && !block_plain_allowed {
        style = Style::Single;
    }
    if style == Style::Single && !single_quoted_allowed {
        style = Style::Double;
    }
    if style == Style::Literal && !block_allowed {
        style = Style::Double;
    }
    style
}

fn write_double_quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        if !is_printable(c) || c == '\u{FEFF}' || is_break(c) || c == '"' || c == '\\' {
            out.push('\\');
            let v = c as u32;
            match v {
                0x00 => out.push('0'),
                0x07 => out.push('a'),
                0x08 => out.push('b'),
                0x09 => out.push('t'),
                0x0A => out.push('n'),
                0x0B => out.push('v'),
                0x0C => out.push('f'),
                0x0D => out.push('r'),
                0x1B => out.push('e'),
                0x22 => out.push('"'),
                0x5C => out.push('\\'),
                0x85 => out.push('N'),
                0xA0 => out.push('_'),
                0x2028 => out.push('L'),
                0x2029 => out.push('P'),
                _ => {
                    if v <= 0xFF {
                        out.push_str(&format!("x{v:02X}"));
                    } else if v <= 0xFFFF {
                        out.push_str(&format!("u{v:04X}"));
                    } else {
                        out.push_str(&format!("U{v:08X}"));
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out.push('"');
}

/// Writes a string scalar (without the trailing newline). `content_indent` is the
/// column of a literal block's lines; `allow_literal` is described on [`string_style`].
pub(crate) fn write_string(out: &mut String, s: &str, content_indent: usize, allow_literal: bool) {
    match string_style(s, allow_literal) {
        Style::Plain => out.push_str(s),
        Style::Single => {
            out.push('\'');
            // A line break other than "\n" is written as is, and the next character
            // starts on the block indent, as libyaml's single quoted writer does.
            let mut breaks = false;
            for c in s.chars() {
                if c == ' ' {
                    out.push(c);
                } else if is_break(c) {
                    if !breaks && c == '\n' {
                        out.push('\n');
                    }
                    out.push(c);
                    breaks = true;
                } else {
                    if breaks {
                        out.push_str(&" ".repeat(content_indent));
                    }
                    if c == '\'' {
                        out.push('\'');
                    }
                    out.push(c);
                    breaks = false;
                }
            }
            out.push('\'');
        }
        Style::Double => write_double_quoted(out, s),
        Style::Literal => {
            out.push('|');
            let first = s.chars().next();
            if matches!(first, Some(' ')) || first.map(is_break).unwrap_or(false) {
                out.push('4');
            }
            let last = s.chars().next_back();
            if !last.map(is_break).unwrap_or(false) {
                out.push('-');
            } else {
                let mut rev = s.chars().rev();
                rev.next();
                match rev.next() {
                    None => out.push('+'),
                    Some(prev) if is_break(prev) => out.push('+'),
                    _ => {}
                }
            }
            out.push('\n');
            let mut breaks = true;
            for c in s.chars() {
                if is_break(c) {
                    out.push(c);
                    breaks = true;
                } else {
                    if breaks {
                        out.push_str(&" ".repeat(content_indent));
                    }
                    out.push(c);
                    breaks = false;
                }
            }
        }
    }
}

/// A string as one line of YAML: plain when that reads back as the same string, otherwise single or double
/// quoted. Text with line breaks is double quoted (`"a\nb"`), never a literal block, so the result can sit
/// after `key: ` or inside a flow value.
pub fn scalar(s: &str) -> String {
    let mut out = String::new();
    write_string(&mut out, s, 0, false);
    out
}
