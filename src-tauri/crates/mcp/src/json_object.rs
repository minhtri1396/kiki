use std::collections::HashMap;

use anyhow::{anyhow, Result};
use kiki_util::json;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

/// A JSON object that keeps keys in file order and stores each value as its original text,
/// so rewriting a settings file leaves everything the user wrote untouched.
#[derive(Debug, Default, Clone)]
pub(crate) struct JsonObject {
    pub keys: Vec<String>,
    pub vals: HashMap<String, String>,
}

impl JsonObject {
    pub fn new() -> JsonObject {
        JsonObject::default()
    }

    pub fn has(&self, key: &str) -> bool {
        self.vals.contains_key(key)
    }

    pub fn set(&mut self, key: &str, raw: &str) {
        if !self.vals.contains_key(key) {
            self.keys.push(key.to_string());
        }
        self.vals.insert(key.to_string(), raw.trim().to_string());
    }

    pub fn compact(&self) -> String {
        let mut buf = String::from("{");
        for (i, key) in self.keys.iter().enumerate() {
            if i > 0 {
                buf.push(',');
            }
            buf.push_str(&quote(key));
            buf.push(':');
            buf.push_str(self.vals[key].trim());
        }
        buf.push('}');
        buf
    }

    pub fn pretty(&self) -> String {
        let mut buf = String::from("{\n");
        for (i, key) in self.keys.iter().enumerate() {
            buf.push_str(&format!("  {}: {}", quote(key), self.vals[key].trim()));
            if i + 1 < self.keys.len() {
                buf.push(',');
            }
            buf.push('\n');
        }
        buf.push_str("}\n");
        buf
    }
}

/// A JSON string literal, HTML-safe like the rest of the file.
fn quote(text: &str) -> String {
    json::marshal(&serde_json::Value::String(text.to_string()))
}

struct Ordered(Vec<(String, String)>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Ordered, A::Error> {
                let mut out = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    let raw: Box<RawValue> = map.next_value()?;
                    out.push((key, raw.get().to_string()));
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(V)
    }
}

pub(crate) fn parse_json_object(data: &[u8]) -> Result<JsonObject> {
    let Ok(text) = std::str::from_utf8(data) else {
        return Err(anyhow!("not valid JSON"));
    };
    let text = text.trim();
    if !text.starts_with('{') {
        // Any complete value that is not an object reads as "not a JSON object".
        if text.starts_with('[') {
            return Err(anyhow!("not a JSON object"));
        }
        let first = serde_json::Deserializer::from_str(text)
            .into_iter::<serde::de::IgnoredAny>()
            .next();
        return match first {
            Some(Ok(_)) => Err(anyhow!("not a JSON object")),
            _ => Err(anyhow!("not valid JSON")),
        };
    }
    let ordered: Ordered = serde_json::from_str(text).map_err(|_| anyhow!("not valid JSON"))?;
    let mut obj = JsonObject::new();
    for (key, val) in ordered.0 {
        if !obj.vals.contains_key(&key) {
            obj.keys.push(key.clone());
        }
        obj.vals.insert(key, val);
    }
    Ok(obj)
}

pub(crate) fn is_null(raw: &str) -> bool {
    raw.trim() == "null"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_and_raw_values() {
        let obj = parse_json_object(b"  {\"b\": [1, 2], \"a\":{ \"x\" : null },\"b\":3}  ").unwrap();
        assert_eq!(obj.keys, vec!["b", "a"]);
        assert_eq!(obj.vals["b"], "3");
        assert_eq!(obj.vals["a"], "{ \"x\" : null }");
        assert_eq!(obj.pretty(), "{\n  \"b\": 3,\n  \"a\": { \"x\" : null }\n}\n");
        assert_eq!(obj.compact(), "{\"b\":3,\"a\":{ \"x\" : null }}");
    }

    #[test]
    fn rejects_non_objects_and_bad_json() {
        assert_eq!(parse_json_object(b"[1]").unwrap_err().to_string(), "not a JSON object");
        assert_eq!(
            parse_json_object(b"\"x\"").unwrap_err().to_string(),
            "not a JSON object"
        );
        assert_eq!(parse_json_object(b"nope").unwrap_err().to_string(), "not valid JSON");
        assert_eq!(parse_json_object(b"{} x").unwrap_err().to_string(), "not valid JSON");
        assert_eq!(parse_json_object(b"{\"a\":").unwrap_err().to_string(), "not valid JSON");
    }

    #[test]
    fn keys_are_written_html_safe() {
        assert_eq!(quote("a<b>&c"), "\"a\\u003cb\\u003e\\u0026c\"");
    }
}
