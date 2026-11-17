//! Reads an agent file's front matter into a [`Header`].
//!
//! Pattern: Template Method. `decode_list` fixes the shape every list field shares (one id, a list of ids, or
//! a list of rows) and each field supplies only how to build an item from an id and from a row.

use kiki_yaml::read::{self, Fields, Node};

use crate::model::{clean_handoffs, custom_guide, Collaborator, Handoff, Repository, Use};

const REPOS_ERR: &str = "repos must be a repository kind or a list";
const PEERS_ERR: &str = "Agents Coordination must be an agent id or a list";
const USES_ERR: &str = "mcps and tools must be an id or a list";

type DecodeResult<T> = std::result::Result<T, String>;

/// The front matter of an agent file, decoded.
#[derive(Default)]
pub(crate) struct Header {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: Option<bool>,
    pub ai_tool: String,
    pub model: String,
    pub repos: Vec<Repository>,
    pub works_with: Vec<Collaborator>,
    pub legacy_teamwork: Vec<Collaborator>,
    pub legacy_works_with: Vec<Collaborator>,
    pub mcps: Vec<Use>,
    pub tools: Vec<Use>,
}

/// Decodes a list field that is one id, a list of ids and rows, or (an error) a map or nested list.
/// `from_id` builds an item from a bare id; `from_row` builds one from a map row, or `None` to skip it.
fn decode_list<T>(
    value: &Node,
    shape_error: &str,
    from_id: fn(&str) -> T,
    from_row: fn(&Node) -> DecodeResult<Option<T>>,
) -> DecodeResult<Vec<T>> {
    match value {
        Node::Scalar { text, .. } => {
            let id = text.trim();
            Ok(if id.is_empty() { Vec::new() } else { vec![from_id(id)] })
        }
        Node::Seq(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Node::Scalar { text, .. } => {
                        let id = text.trim();
                        if !id.is_empty() {
                            out.push(from_id(id));
                        }
                    }
                    Node::Map(_) => out.extend(from_row(item)?),
                    Node::Seq(_) => return Err(shape_error.to_string()),
                }
            }
            Ok(out)
        }
        Node::Map(_) => Err(shape_error.to_string()),
    }
}

/// Decodes `repos`: a kind, a list of kinds, or a list of kind and description notes.
fn decode_repos(value: &Node) -> DecodeResult<Vec<Repository>> {
    decode_list(
        value,
        REPOS_ERR,
        |kind| Repository {
            kind: kind.to_string(),
            ..Default::default()
        },
        |item| {
            let row = Fields::of(item)?;
            let id = row.string("id")?;
            let kind_key = row.string("kind")?;
            let guide = row.string("guide")?;
            let description = row.string("description")?;
            let readonly = row.bool("readonly")?;
            let mut kind = id.trim().to_string();
            if kind.is_empty() {
                kind = kind_key.trim().to_string();
            }
            if kind.is_empty() {
                return Ok(None);
            }
            Ok(Some(Repository {
                kind,
                guide: custom_guide(&guide),
                description: description.trim().to_string(),
                readonly,
            }))
        },
    )
}

/// Decodes the agents this agent works with: an id, a list of ids, or a list of ids
/// with handoffs. A handoff is a when and send pair. An old description, or the older
/// key collaboration, is read as one handoff with that text to send.
fn decode_peers(value: &Node) -> DecodeResult<Vec<Collaborator>> {
    decode_list(
        value,
        PEERS_ERR,
        |id| Collaborator {
            id: id.to_string(),
            handoffs: Vec::new(),
        },
        |item| {
            let row = Fields::of(item)?;
            let id_raw = row.string("id")?;
            let description = row.string("description")?;
            let collaboration = row.string("collaboration")?;
            let mut pairs = decode_handoffs(&row)?;
            let id = id_raw.trim();
            if id.is_empty() {
                return Ok(None);
            }
            if pairs.is_empty() {
                let mut note = description.trim().to_string();
                if note.is_empty() {
                    note = collaboration.trim().to_string();
                }
                pairs = vec![Handoff {
                    send: note,
                    ..Default::default()
                }];
            }
            Ok(Some(Collaborator {
                id: id.to_string(),
                handoffs: clean_handoffs(&pairs),
            }))
        },
    )
}

/// The `handoffs` list of one collaborator row; a missing key or null is no handoffs.
fn decode_handoffs(row: &Fields) -> DecodeResult<Vec<Handoff>> {
    let mut pairs = Vec::new();
    match row.get("handoffs") {
        None => {}
        Some(n) if n.is_null() => {}
        Some(Node::Seq(list)) => {
            for pair in list {
                let f = Fields::of(pair)?;
                pairs.push(Handoff {
                    when: f.string("when")?,
                    send: f.string("send")?,
                    reply: f.string("reply")?,
                    max_loops: f.int("max_loops")?,
                });
            }
        }
        Some(_) => return Err("cannot unmarshal handoffs into a list".to_string()),
    }
    Ok(pairs)
}

/// Decodes `mcps` or `tools`: an id, a list of ids, or a list of id and description notes.
fn decode_uses(value: &Node) -> DecodeResult<Vec<Use>> {
    decode_list(
        value,
        USES_ERR,
        |id| Use {
            id: id.to_string(),
            ..Default::default()
        },
        |item| {
            let row = Fields::of(item)?;
            let id = row.string("id")?;
            let description = row.string("description")?;
            let board_id = row.string("board_id")?;
            let board_name = row.string("board_name")?;
            let space_id = row.string("space_id")?;
            let space_key = row.string("space_key")?;
            let space_name = row.string("space_name")?;
            let parent_id = row.string("parent_id")?;
            let parent_title = row.string("parent_title")?;
            let id = id.trim();
            if id.is_empty() {
                return Ok(None);
            }
            Ok(Some(Use {
                id: id.to_string(),
                description: description.trim().to_string(),
                board_id: board_id.trim().to_string(),
                board_name: board_name.trim().to_string(),
                space_id: space_id.trim().to_string(),
                space_key: space_key.trim().to_string(),
                space_name: space_name.trim().to_string(),
                parent_id: parent_id.trim().to_string(),
                parent_title: parent_title.trim().to_string(),
            }))
        },
    )
}

/// Decodes a YAML list field. A null leaves it empty without calling the decoder,
/// as yaml.v3 does.
fn list_field<T>(fields: &Fields, key: &str, decode: fn(&Node) -> DecodeResult<Vec<T>>) -> DecodeResult<Vec<T>> {
    match fields.get(key) {
        None => Ok(Vec::new()),
        Some(n) if n.is_null() => Ok(Vec::new()),
        Some(n) => decode(n),
    }
}

fn decode_header(root: Option<Node>) -> DecodeResult<Header> {
    let Some(root) = root else { return Ok(Header::default()) };
    let fields = Fields::of(&root)?;
    let enabled = match fields.get("enabled") {
        Some(n) => n.to_bool_field()?,
        None => None,
    };
    Ok(Header {
        id: fields.string("id")?,
        name: fields.string("name")?,
        description: fields.string("description")?,
        enabled,
        ai_tool: fields.string("ai_tool")?,
        model: fields.string("model")?,
        repos: list_field(&fields, "repos", decode_repos)?,
        works_with: list_field(&fields, "agents_coordination", decode_peers)?,
        legacy_teamwork: list_field(&fields, "teamwork", decode_peers)?,
        legacy_works_with: list_field(&fields, "works_with", decode_peers)?,
        mcps: list_field(&fields, "mcps", decode_uses)?,
        tools: list_field(&fields, "tools", decode_uses)?,
    })
}

/// Splits `text` at its front matter and decodes it. The older `teamwork` and `works_with` keys fill
/// `works_with` when `agents_coordination` is empty.
pub(crate) fn parse(text: &str) -> DecodeResult<(Header, String)> {
    let Some((front, body)) = split(text) else {
        return Err("agent file has no front matter".to_string());
    };
    let root = read::parse(front)?;
    let mut meta = decode_header(root)?;
    if meta.works_with.is_empty() {
        meta.works_with = std::mem::take(&mut meta.legacy_teamwork);
    }
    if meta.works_with.is_empty() {
        meta.works_with = std::mem::take(&mut meta.legacy_works_with);
    }
    meta.legacy_teamwork = Vec::new();
    meta.legacy_works_with = Vec::new();
    Ok((meta, body.to_string()))
}

/// Separates `---` fenced front matter from the markdown body.
pub(crate) fn split(text: &str) -> Option<(&str, &str)> {
    const FENCE: &str = "---";
    let rest = text.strip_prefix("---\n")?;
    let marker = format!("\n{FENCE}\n");
    let end = rest.find(&marker)?;
    let mut body = &rest[end + marker.len()..];
    if let Some(stripped) = body.strip_prefix('\n') {
        body = stripped;
    }
    Some((&rest[..end], body))
}
