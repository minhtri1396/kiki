//! Reading Atlassian JSON bodies leniently and trimming text for error messages.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

/// Decodes a body into a struct: `null` leaves the zero value, anything but an object is an error.
pub(crate) fn decode<T: DeserializeOwned + Default>(body: &[u8]) -> std::result::Result<T, String> {
    let text = String::from_utf8_lossy(body);
    if text.trim().is_empty() {
        return Err("unexpected end of JSON input".to_string());
    }
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    value_into(value)
}

/// Same as [`decode`] for a value that is already parsed.
pub(crate) fn value_into<T: DeserializeOwned + Default>(value: serde_json::Value) -> std::result::Result<T, String> {
    let kind = match &value {
        serde_json::Value::Null => return Ok(T::default()),
        serde_json::Value::Object(_) => return serde_json::from_value(value).map_err(|e| e.to_string()),
        serde_json::Value::Array(_) => "array",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
    };
    // The wording is part of the messages users already see, so it stays as is.
    Err(format!("json: cannot unmarshal {kind} into Go value of type struct"))
}

/// `null` reads as the zero value, the same as a missing field.
pub(crate) fn nd<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// A JSON id that may be a string or a bare number. Keeps the text.
#[derive(Default)]
pub(crate) struct FlexId {
    pub raw: String,
}

impl<'de> Deserialize<'de> for FlexId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        let raw = match value {
            serde_json::Value::Null => String::new(),
            serde_json::Value::String(text) => text,
            other => other.to_string(),
        };
        Ok(FlexId { raw })
    }
}

/// Trims, joins lines with spaces, and cuts to 180 bytes for an error message.
pub(crate) fn one_line(text: &str) -> String {
    let text = text.trim().replace(['\n', '\r'], " ");
    clip_bytes(&text, 180)
}

/// Cuts to at most `max` bytes, backing up to a character boundary.
pub(crate) fn clip_bytes(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_joins_and_clips() {
        assert_eq!(one_line("  a\r\nb  "), "a  b");
        assert_eq!(clip_bytes("héllo", 2), "h");
        assert_eq!(one_line(&"x".repeat(300)).len(), 180);
    }

    #[test]
    fn decode_reads_null_as_default_and_rejects_non_objects() {
        #[derive(serde::Deserialize, Default)]
        struct Body {
            #[serde(default, deserialize_with = "nd")]
            name: String,
        }
        assert_eq!(decode::<Body>(b"null").unwrap().name, "");
        assert_eq!(decode::<Body>(b"{\"name\":null}").unwrap().name, "");
        assert_eq!(decode::<Body>(b" ").err().unwrap(), "unexpected end of JSON input");
        assert_eq!(
            decode::<Body>(b"[1]").err().unwrap(),
            "json: cannot unmarshal array into Go value of type struct"
        );
    }
}
