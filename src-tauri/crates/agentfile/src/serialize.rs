//! Writes a [`Header`] and body as an agent markdown file.
//!
//! Pattern: Template Method. `ids_or_rows` fixes how a list field is written (bare ids unless some entry has
//! a note, then rows) and each field supplies only what makes an entry "noted" and how its row looks.

use kiki_yaml::{Mapping, Value};

use crate::model::{clean_handoffs, custom_guide, Collaborator, Repository, Use};
use crate::parse::Header;

/// Writes `items` as a sequence of bare ids, or as rows when any item `has_note`.
fn ids_or_rows<T>(
    items: &[T],
    id_of: fn(&T) -> &str,
    has_note: impl Fn(&T) -> bool,
    row_of: impl Fn(&T) -> Mapping,
) -> Value {
    if !items.iter().any(has_note) {
        return Value::Seq(items.iter().map(|item| Value::from(id_of(item))).collect());
    }
    Value::Seq(items.iter().map(|item| row_of(item).into_value()).collect())
}

/// Writes `repos`: kinds only, or id rows when any repository has a note.
fn repos_value(repos: &[Repository]) -> Value {
    ids_or_rows(
        repos,
        |r| &r.kind,
        |r| !r.description.trim().is_empty() || r.readonly || !custom_guide(&r.guide).is_empty(),
        |r| {
            Mapping::new()
                .set("id", r.kind.as_str())
                .set_unless_empty("guide", &custom_guide(&r.guide))
                .set_unless_empty("description", r.description.trim())
                .set_if(r.readonly, "readonly", true)
        },
    )
}

/// Writes the agents this agent works with: ids only, or id rows when any has a message.
fn peers_value(peers: &[Collaborator]) -> Value {
    ids_or_rows(
        peers,
        |p| &p.id,
        |p| !clean_handoffs(&p.handoffs).is_empty(),
        |p| {
            let handoffs = clean_handoffs(&p.handoffs);
            let row = Mapping::new().set("id", p.id.as_str());
            if handoffs.is_empty() {
                return row;
            }
            let pairs = handoffs
                .into_iter()
                .map(|h| {
                    Mapping::new()
                        .set_unless_empty("when", &h.when)
                        .set("send", h.send)
                        .set_unless_empty("reply", &h.reply)
                        .set_if(h.max_loops != 0, "max_loops", h.max_loops)
                        .into_value()
                })
                .collect();
            row.set("handoffs", Value::Seq(pairs))
        },
    )
}

/// Writes `mcps` or `tools`: ids only, or id rows when any entry has a note.
fn uses_value(uses: &[Use]) -> Value {
    ids_or_rows(
        uses,
        |u| &u.id,
        Use::detailed,
        |u| {
            let fields = [
                ("description", &u.description),
                ("board_id", &u.board_id),
                ("board_name", &u.board_name),
                ("space_id", &u.space_id),
                ("space_key", &u.space_key),
                ("space_name", &u.space_name),
                ("parent_id", &u.parent_id),
                ("parent_title", &u.parent_title),
            ];
            fields
                .into_iter()
                .fold(Mapping::new().set("id", u.id.as_str()), |row, (key, value)| {
                    row.set_unless_empty(key, value.trim())
                })
        },
    )
}

/// The YAML of a front matter header, as `yaml.Marshal` writes it.
fn marshal_header(meta: &Header) -> String {
    let mut map = Mapping::new()
        .set("id", meta.id.as_str())
        .set("name", meta.name.as_str())
        .set("description", meta.description.as_str());
    if let Some(enabled) = meta.enabled {
        map = map.set("enabled", enabled);
    }
    map.set("ai_tool", meta.ai_tool.as_str())
        .set("model", meta.model.as_str())
        .set("repos", repos_value(&meta.repos))
        .set_if(
            !meta.works_with.is_empty(),
            "agents_coordination",
            peers_value(&meta.works_with),
        )
        .set_if(
            !meta.legacy_teamwork.is_empty(),
            "teamwork",
            peers_value(&meta.legacy_teamwork),
        )
        .set_if(
            !meta.legacy_works_with.is_empty(),
            "works_with",
            peers_value(&meta.legacy_works_with),
        )
        .set_if(!meta.mcps.is_empty(), "mcps", uses_value(&meta.mcps))
        .set_if(!meta.tools.is_empty(), "tools", uses_value(&meta.tools))
        .emit()
}

/// The whole file: `---` fenced front matter, a blank line, then the body ending in one newline.
pub(crate) fn format(meta: &Header, body: &str) -> String {
    let raw = marshal_header(meta);
    let body = format!("{}\n", body.trim_end_matches('\n'));
    format!("---\n{raw}---\n\n{body}")
}
