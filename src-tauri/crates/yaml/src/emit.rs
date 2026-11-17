//! Emitter: writes a [`Value`] tree as the text yaml.v3 produces.
//!
//! The layout is yaml.v3's: 4-space indent, block sequences indented under their key, `- ` items with their
//! map fields two columns in, `[]` and `{}` for empty collections, and the scalar style rules of
//! [`scalar`](crate::scalar()). Maps keep the order entries were added in, like the fields of a struct.
//!
//! ```
//! use kiki_yaml::Mapping;
//!
//! let text = Mapping::new()
//!     .set("id", "backend")
//!     .set("services", vec!["api".into(), "worker".into()])
//!     .set_unless_empty("index", "")
//!     .emit();
//! assert_eq!(text, "id: backend\nservices:\n    - api\n    - worker\n");
//! ```

use crate::scalar::write_string;

/// A YAML node to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Bool(bool),
    Int(i64),
    /// A block sequence; empty is written `[]`.
    Seq(Vec<Value>),
    /// A block mapping in insertion order; empty is written `{}`.
    Map(Vec<(String, Value)>),
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::Str(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::Str(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Value {
        Value::Int(n)
    }
}

impl From<Vec<Value>> for Value {
    fn from(items: Vec<Value>) -> Value {
        Value::Seq(items)
    }
}

impl From<Mapping> for Value {
    fn from(map: Mapping) -> Value {
        map.into_value()
    }
}

/// Builder for an ordered mapping: add entries in the order they must appear, then [`emit`](Mapping::emit)
/// it as a document or turn it into a nested [`Value`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mapping {
    entries: Vec<(String, Value)>,
}

impl Mapping {
    pub fn new() -> Mapping {
        Mapping::default()
    }

    /// Adds an entry.
    pub fn set(mut self, key: impl Into<String>, value: impl Into<Value>) -> Mapping {
        self.entries.push((key.into(), value.into()));
        self
    }

    /// Adds a string entry only when `value` is not empty (yaml.v3 `omitempty`).
    pub fn set_unless_empty(self, key: impl Into<String>, value: &str) -> Mapping {
        if value.is_empty() {
            self
        } else {
            self.set(key, value)
        }
    }

    /// Adds an entry only when `condition` holds.
    pub fn set_if(self, condition: bool, key: impl Into<String>, value: impl Into<Value>) -> Mapping {
        if condition {
            self.set(key, value)
        } else {
            self
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn into_value(self) -> Value {
        Value::Map(self.entries)
    }

    /// The document text for this mapping, as `yaml.Marshal` writes a struct.
    pub fn emit(&self) -> String {
        emit(&self.entries)
    }
}

/// Writes a top-level mapping. An empty mapping gives an empty string.
pub fn emit(entries: &[(String, Value)]) -> String {
    let mut out = String::new();
    write_map(&mut out, entries, 0, false);
    out
}

/// yaml.v3 aligns a nested block to the next multiple of the 4-space indent.
fn next_indent(indent: usize) -> usize {
    4 * ((indent + 4) / 4)
}

/// Writes a scalar value without the trailing newline. `content_indent` is where a literal block's lines go.
fn write_scalar(out: &mut String, value: &Value, content_indent: usize) {
    match value {
        Value::Str(s) => write_string(out, s, content_indent, true),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Seq(_) | Value::Map(_) => {}
    }
}

/// Ends the current line. A literal block already ends with the line break of its last line.
fn end_line(out: &mut String) {
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

fn write_map(out: &mut String, entries: &[(String, Value)], indent: usize, first_inline: bool) {
    for (i, (key, value)) in entries.iter().enumerate() {
        if !(i == 0 && first_inline) {
            out.push_str(&" ".repeat(indent));
        }
        write_string(out, key, 0, false);
        out.push(':');
        match value {
            Value::Seq(items) if items.is_empty() => out.push_str(" []\n"),
            Value::Map(children) if children.is_empty() => out.push_str(" {}\n"),
            Value::Seq(items) => {
                out.push('\n');
                write_seq(out, items, next_indent(indent), false);
            }
            Value::Map(children) => {
                out.push('\n');
                write_map(out, children, next_indent(indent), false);
            }
            scalar => {
                out.push(' ');
                write_scalar(out, scalar, next_indent(indent));
                end_line(out);
            }
        }
    }
}

fn write_seq(out: &mut String, items: &[Value], indent: usize, first_inline: bool) {
    for (i, item) in items.iter().enumerate() {
        if !(i == 0 && first_inline) {
            out.push_str(&" ".repeat(indent));
        }
        out.push_str("- ");
        match item {
            Value::Map(entries) if entries.is_empty() => out.push_str("{}\n"),
            Value::Map(entries) => write_map(out, entries, indent + 2, true),
            Value::Seq(inner) if inner.is_empty() => out.push_str("[]\n"),
            Value::Seq(inner) => write_seq(out, inner, indent + 2, true),
            scalar => {
                write_scalar(out, scalar, indent + 2);
                end_line(out);
            }
        }
    }
}

#[cfg(test)]
mod tests;
