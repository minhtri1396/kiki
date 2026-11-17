//! Decoding of one JSON line a CLI prints (Cursor and Claude stream-json, Codex items).
//!
//! A field of the wrong JSON type rejects the whole line, which is then treated as plain text.
//! `null` and missing fields are empty values. The helpers below keep exactly that behaviour.

use serde_json::{Map, Value};

#[derive(Default)]
pub(super) struct Delta {
    pub kind: String,
    pub text: String,
    pub thinking: String,
}

#[derive(Default)]
pub(super) struct Block {
    pub kind: String,
    pub text: String,
    pub thinking: String,
}

#[derive(Default)]
pub(super) struct Event {
    pub kind: String,
    pub subtype: String,
    pub text: String,
    pub thinking: String,
    pub result: String,
    pub is_error: bool,
    pub content: Vec<Block>,
    pub event_delta: Delta,
    pub delta: Delta,
    pub item_kind: String,
    pub item_text: String,
}

fn get_str(obj: &Map<String, Value>, key: &str) -> Option<String> {
    match obj.get(key) {
        None | Some(Value::Null) => Some(String::new()),
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn get_bool(obj: &Map<String, Value>, key: &str) -> Option<bool> {
    match obj.get(key) {
        None | Some(Value::Null) => Some(false),
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

/// An object-typed field: missing/null is an empty object, anything else that is not an object is a decode error.
fn get_obj<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<Option<&'a Map<String, Value>>> {
    match obj.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::Object(m)) => Some(Some(m)),
        _ => None,
    }
}

fn parse_delta(obj: Option<&Map<String, Value>>) -> Option<Delta> {
    let Some(obj) = obj else { return Some(Delta::default()) };
    Some(Delta {
        kind: get_str(obj, "type")?,
        text: get_str(obj, "text")?,
        thinking: get_str(obj, "thinking")?,
    })
}

/// `message.content`: an array of blocks (a `null` entry is an empty block).
fn parse_content(message: &Map<String, Value>) -> Option<Vec<Block>> {
    let mut content = Vec::new();
    match message.get("content") {
        None | Some(Value::Null) => {}
        Some(Value::Array(items)) => {
            for item in items {
                match item {
                    Value::Null => content.push(Block::default()),
                    Value::Object(b) => content.push(Block {
                        kind: get_str(b, "type")?,
                        text: get_str(b, "text")?,
                        thinking: get_str(b, "thinking")?,
                    }),
                    _ => return None,
                }
            }
        }
        _ => return None,
    }
    Some(content)
}

/// Decodes one line, or `None` when it is not a well-typed JSON object.
pub(super) fn parse_event(line: &str) -> Option<Event> {
    let value: Value = serde_json::from_str(line).ok()?;
    let Value::Object(obj) = &value else { return None };
    let mut event = Event {
        kind: get_str(obj, "type")?,
        subtype: get_str(obj, "subtype")?,
        text: get_str(obj, "text")?,
        thinking: get_str(obj, "thinking")?,
        result: get_str(obj, "result")?,
        is_error: get_bool(obj, "is_error")?,
        ..Default::default()
    };
    if let Some(message) = get_obj(obj, "message")? {
        event.content = parse_content(message)?;
    }
    if let Some(ev) = get_obj(obj, "event")? {
        event.event_delta = parse_delta(get_obj(ev, "delta")?)?;
        // The inner type is decoded for its type check only; it is never read.
        get_str(ev, "type")?;
    }
    event.delta = parse_delta(get_obj(obj, "delta")?)?;
    if let Some(item) = get_obj(obj, "item")? {
        event.item_kind = get_str(item, "type")?;
        event.item_text = get_str(item, "text")?;
    }
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_null_fields_are_empty() {
        let event = parse_event(r#"{"type":"result","subtype":null,"result":"ok"}"#).unwrap();
        assert_eq!(
            (event.kind.as_str(), event.subtype.as_str(), event.result.as_str()),
            ("result", "", "ok")
        );
        assert!(!event.is_error);
    }

    #[test]
    fn a_wrong_type_rejects_the_line() {
        assert!(parse_event(r#"{"type":5}"#).is_none());
        assert!(parse_event(r#"{"type":"result","is_error":"yes"}"#).is_none());
        assert!(parse_event(r#"{"type":"assistant","message":{"content":[1]}}"#).is_none());
        assert!(parse_event(r#"["type"]"#).is_none());
        assert!(parse_event("not json").is_none());
    }

    #[test]
    fn content_blocks_and_nested_deltas_decode() {
        let event = parse_event(
            r#"{"type":"assistant","message":{"content":[null,{"type":"text","text":"hi"}]},"event":{"type":"x","delta":{"type":"text_delta","text":"d"}},"item":{"type":"reasoning","text":"t"}}"#,
        )
        .unwrap();
        assert_eq!(event.content.len(), 2);
        assert_eq!(event.content[1].text, "hi");
        assert_eq!(event.event_delta.kind, "text_delta");
        assert_eq!((event.item_kind.as_str(), event.item_text.as_str()), ("reasoning", "t"));
    }
}
