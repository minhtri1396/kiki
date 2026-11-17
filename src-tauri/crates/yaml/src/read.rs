//! Reader: parses YAML text into a node tree and decodes it with yaml.v3 field rules.
//!
//! The tree keeps the raw scalar text and whether it was plain, because yaml.v3 decodes `board_id: 12`
//! into a string field as the text `12`. [`Fields`] then reads one mapping like a struct: a missing key or
//! null is the zero value, numbers and booleans read as their text, repeated keys are an error.

use std::collections::HashMap;

use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

use crate::resolve::{resolve, Resolved};

/// 2^63: the first float that no longer fits an `i64` (and the negation of the first one below).
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// One YAML node. A scalar keeps its raw text and whether it was written plain
/// (an unquoted scalar can be null, a bool or a number; a quoted one is a string).
#[derive(Debug, Clone)]
pub enum Node {
    Scalar { text: String, plain: bool },
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

struct Builder {
    stack: Vec<Frame>,
    anchors: HashMap<usize, Node>,
    root: Option<Node>,
    error: Option<String>,
}

enum Frame {
    Seq(usize, Vec<Node>),
    Map(usize, Vec<(Node, Node)>, Option<Node>),
}

impl Builder {
    fn push_value(&mut self, node: Node) {
        match self.stack.last_mut() {
            None => {
                if self.root.is_none() {
                    self.root = Some(node);
                }
            }
            Some(Frame::Seq(_, items)) => items.push(node),
            Some(Frame::Map(_, pairs, pending)) => match pending.take() {
                None => *pending = Some(node),
                Some(key) => pairs.push((key, node)),
            },
        }
    }

    fn finish(&mut self, anchor: usize, node: Node) {
        if anchor != 0 {
            self.anchors.insert(anchor, node.clone());
        }
        self.push_value(node);
    }
}

impl MarkedEventReceiver for Builder {
    fn on_event(&mut self, ev: Event, _mark: Marker) {
        match ev {
            Event::Scalar(text, style, anchor, tag) => {
                let forced_str = tag
                    .as_ref()
                    .map(|t| t.handle == "!!" && t.suffix == "str")
                    .unwrap_or(false);
                let plain = style == TScalarStyle::Plain && !forced_str;
                self.finish(anchor, Node::Scalar { text, plain });
            }
            Event::SequenceStart(anchor, _) => self.stack.push(Frame::Seq(anchor, Vec::new())),
            Event::SequenceEnd => {
                if let Some(Frame::Seq(anchor, items)) = self.stack.pop() {
                    self.finish(anchor, Node::Seq(items));
                }
            }
            Event::MappingStart(anchor, _) => self.stack.push(Frame::Map(anchor, Vec::new(), None)),
            Event::MappingEnd => {
                if let Some(Frame::Map(anchor, pairs, _)) = self.stack.pop() {
                    self.finish(anchor, Node::Map(pairs));
                }
            }
            Event::Alias(id) => match self.anchors.get(&id).cloned() {
                Some(node) => self.push_value(node),
                None => self.error = Some("unknown anchor".to_string()),
            },
            _ => {}
        }
    }
}

/// Parses the first YAML document in `text`. `Ok(None)` means the text has no
/// content, which yaml.v3 decodes as the zero value without an error.
pub fn parse(text: &str) -> Result<Option<Node>, String> {
    let mut builder = Builder {
        stack: Vec::new(),
        anchors: HashMap::new(),
        root: None,
        error: None,
    };
    let mut parser = Parser::new_from_str(text);
    parser.load(&mut builder, false).map_err(|e| e.to_string())?;
    if let Some(err) = builder.error {
        return Err(err);
    }
    Ok(builder.root)
}

impl Node {
    /// A null node: a plain scalar that resolves to null (`~`, `null`, or empty).
    pub fn is_null(&self) -> bool {
        match self {
            Node::Scalar { text, plain } => *plain && resolve(text) == Resolved::Null,
            _ => false,
        }
    }

    /// Decodes into a string field: a null leaves the field empty, any other
    /// scalar is its raw text, a list or mapping is an error.
    pub fn to_string_field(&self) -> Result<String, String> {
        match self {
            Node::Scalar { text, plain } => {
                if *plain && resolve(text) == Resolved::Null {
                    return Ok(String::new());
                }
                Ok(text.clone())
            }
            Node::Seq(_) => Err("cannot unmarshal !!seq into string".to_string()),
            Node::Map(_) => Err("cannot unmarshal !!map into string".to_string()),
        }
    }

    /// Decodes into an optional bool field. `None` means null.
    pub fn to_bool_field(&self) -> Result<Option<bool>, String> {
        match self {
            Node::Scalar { text, plain } => {
                if *plain {
                    match resolve(text) {
                        Resolved::Null => return Ok(None),
                        Resolved::Bool(b) => return Ok(Some(b)),
                        Resolved::Str => {}
                        _ => return Err(format!("cannot unmarshal `{text}` into bool")),
                    }
                }
                // yaml.v3 accepts the YAML 1.1 spellings when the target is a bool.
                match text.as_str() {
                    "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" => Ok(Some(true)),
                    "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF" => Ok(Some(false)),
                    _ => Err(format!("cannot unmarshal `{text}` into bool")),
                }
            }
            _ => Err("cannot unmarshal a list or mapping into bool".to_string()),
        }
    }

    /// Decodes into an integer field. Null leaves it zero.
    pub fn to_int_field(&self) -> Result<i64, String> {
        match self {
            Node::Scalar { text, plain } => {
                if !*plain {
                    return Err(format!("cannot unmarshal `{text}` into int"));
                }
                match resolve(text) {
                    Resolved::Null => Ok(0),
                    Resolved::Int(v) => i64::try_from(v).map_err(|_| format!("cannot unmarshal `{text}` into int")),
                    Resolved::Float(f) => {
                        if f.is_nan() || !(-I64_LIMIT..I64_LIMIT).contains(&f) {
                            Err(format!("cannot unmarshal `{text}` into int"))
                        } else {
                            Ok(f as i64)
                        }
                    }
                    _ => Err(format!("cannot unmarshal `{text}` into int")),
                }
            }
            _ => Err("cannot unmarshal a list or mapping into int".to_string()),
        }
    }
}

/// The keys of a mapping decoded into a struct. A null decodes as an empty
/// struct; yaml.v3 rejects any other non-mapping and any repeated key.
pub struct Fields<'a> {
    map: HashMap<String, &'a Node>,
}

impl<'a> Fields<'a> {
    pub fn of(node: &'a Node) -> Result<Fields<'a>, String> {
        let pairs = match node {
            Node::Map(pairs) => pairs,
            n if n.is_null() => return Ok(Fields { map: HashMap::new() }),
            Node::Scalar { .. } => return Err("cannot unmarshal a scalar into a struct".to_string()),
            Node::Seq(_) => return Err("cannot unmarshal !!seq into a struct".to_string()),
        };
        let mut map = HashMap::new();
        for (key, value) in pairs {
            let name = match key {
                Node::Scalar { text, .. } => text.clone(),
                _ => return Err("mapping key must be a scalar".to_string()),
            };
            if map.insert(name.clone(), value).is_some() {
                return Err(format!("mapping key {name:?} already defined"));
            }
        }
        Ok(Fields { map })
    }

    pub fn get(&self, key: &str) -> Option<&'a Node> {
        self.map.get(key).copied()
    }

    pub fn string(&self, key: &str) -> Result<String, String> {
        match self.get(key) {
            Some(n) => n.to_string_field(),
            None => Ok(String::new()),
        }
    }

    pub fn int(&self, key: &str) -> Result<i64, String> {
        match self.get(key) {
            Some(n) => n.to_int_field(),
            None => Ok(0),
        }
    }

    pub fn bool(&self, key: &str) -> Result<bool, String> {
        match self.get(key) {
            Some(n) => Ok(n.to_bool_field()?.unwrap_or(false)),
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests;
