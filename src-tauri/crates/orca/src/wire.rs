//! Decoding the JSON the Orca CLI and runtime send back: tolerant of `null` and missing fields, strict about shape.

use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Decodes a JSON `null` into the type's default.
pub(crate) fn nd<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Parses a JSON object into a struct: `null` gives the default value, anything but an object is an error.
pub(crate) fn parse<T: DeserializeOwned + Default>(raw: &[u8]) -> std::result::Result<T, serde_json::Error> {
    let value: Value = serde_json::from_slice(raw)?;
    match value {
        Value::Null => Ok(T::default()),
        Value::Object(_) => serde_json::from_value(value),
        _ => Err(<serde_json::Error as serde::de::Error>::custom(
            "json: cannot unmarshal into Go value of type struct",
        )),
    }
}

/// The `error` object of a failed reply.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub(crate) struct ErrorBody {
    #[serde(deserialize_with = "nd")]
    pub(crate) code: String,
    #[serde(deserialize_with = "nd")]
    pub(crate) message: String,
}

/// The reply shape every `orca ... --json` command shares: `{ok, error, result}`.
#[derive(Deserialize, Default)]
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
pub(crate) struct Envelope<T> {
    #[serde(deserialize_with = "nd")]
    pub(crate) ok: bool,
    #[serde(deserialize_with = "nd")]
    pub(crate) error: ErrorBody,
    #[serde(deserialize_with = "nd")]
    pub(crate) result: T,
}

/// Whether the reply says `"ok": true`.
pub(crate) fn json_ok(out: &[u8]) -> bool {
    parse::<Envelope<IgnoredAny>>(out).map(|env| env.ok).unwrap_or(false)
}

/// Parses a `{ok, error, result}` reply and returns its result; a reply that is not JSON gets `context` in front.
pub(crate) fn parse_result<T: DeserializeOwned + Default>(out: &[u8], context: &str) -> anyhow::Result<T> {
    let env: Envelope<T> = parse(out).map_err(|e| anyhow::anyhow!("{context}: {e}"))?;
    Ok(env.result)
}

/// Whether the reply says the thing it was asked to remove is already gone.
pub(crate) fn orca_gone(out: &[u8]) -> bool {
    match parse::<Envelope<IgnoredAny>>(out) {
        Ok(env) => orca_gone_text(&env.error.code) || orca_gone_text(&env.error.message),
        Err(_) => false,
    }
}

/// Whether an error code or message means "already gone".
pub(crate) fn orca_gone_text(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return false;
    }
    text == "terminal_handle_stale" || text.contains("not_found") || text.contains("not found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_null_tolerant_and_object_strict() {
        let env: Envelope<ErrorBody> = parse(b"null").unwrap();
        assert!(!env.ok);
        assert!(parse::<Envelope<IgnoredAny>>(b"[1]").is_err());
        assert!(parse::<Envelope<IgnoredAny>>(b"not json").is_err());
        let env: Envelope<ErrorBody> = parse(br#"{"ok":true,"result":null,"error":{"code":null}}"#).unwrap();
        assert!(env.ok && env.result.code.is_empty() && env.error.code.is_empty());
        assert!(json_ok(br#"{"ok":true}"#) && !json_ok(b"[]"));
    }

    #[test]
    fn gone_replies() {
        assert!(orca_gone(
            br#"{"ok":false,"error":{"code":"runtime_error","message":"Project host setup not found: x"}}"#
        ));
        assert!(!orca_gone(
            br#"{"ok":false,"error":{"code":"runtime_error","message":"still open"}}"#
        ));
        assert!(!orca_gone(b"junk"));
        assert!(orca_gone_text(" Terminal_Handle_Stale "));
    }

    #[test]
    fn parse_result_names_the_context() {
        let err = parse_result::<ErrorBody>(b"[]", "read thing").unwrap_err().to_string();
        assert!(err.starts_with("read thing: "), "err = {err}");
    }
}
