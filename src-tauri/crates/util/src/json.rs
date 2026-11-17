//! JSON text helpers. KIKI keeps writing HTML-safe JSON (`<`, `>`, `&`, U+2028, U+2029 escaped inside strings)
//! so settings files that earlier versions wrote do not change when KIKI saves them again.

use serde_json::Value;

/// Escapes `<`, `>`, `&`, U+2028 and U+2029 inside strings.
/// Those characters can only appear inside JSON strings, so a text replace is safe.
fn escape(s: String) -> String {
    if !s.contains(['<', '>', '&', '\u{2028}', '\u{2029}']) {
        return s;
    }
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}

/// Compact JSON, HTML-safe.
pub fn marshal(v: &Value) -> String {
    escape(serde_json::to_string(v).unwrap_or_else(|_| "null".to_string()))
}

/// JSON indented by two spaces, HTML-safe.
pub fn marshal_indent(v: &Value) -> String {
    escape(serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn escapes_html_characters_inside_strings() {
        assert_eq!(marshal(&json!({"a": "<x>&"})), "{\"a\":\"\\u003cx\\u003e\\u0026\"}");
        assert_eq!(marshal_indent(&json!({"a": 1})), "{\n  \"a\": 1\n}");
    }
}
