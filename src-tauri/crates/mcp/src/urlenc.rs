//! URL escaping for query strings and path segments.

/// Encodes a query string with keys sorted and both sides escaped (`a b` becomes `a+b`).
pub(crate) fn encode_query(mut values: Vec<(&str, String)>) -> String {
    values.sort_by(|a, b| a.0.cmp(b.0));
    values
        .iter()
        .map(|(k, v)| format!("{}={}", query_escape(k), query_escape(v)))
        .collect::<Vec<_>>()
        .join("&")
}

pub(crate) fn query_escape(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Escapes one path segment (a space becomes `%20`, `/` becomes `%2F`).
pub(crate) fn path_escape(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'$'
            | b'&'
            | b'+'
            | b'='
            | b':'
            | b'@' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_queries_and_paths() {
        assert_eq!(path_escape("a b/c"), "a%20b%2Fc");
        assert_eq!(query_escape("a b&c~"), "a+b%26c~");
        assert_eq!(encode_query(vec![("b", "1".into()), ("a", "x y".into())]), "a=x+y&b=1");
    }
}
