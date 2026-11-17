//! The "MCP servers" and "Tools" sections: what the agent does with each one.

use kiki_agentdef as agentdef;

/// Writes `heading`, then each used item with its place and description. `label` introduces the
/// description when it is not empty. Nothing is written when no item has an id.
pub(super) fn write(b: &mut String, heading: &str, label: &str, items: &[agentdef::Use]) {
    let list: Vec<agentdef::Use> = items.iter().filter_map(trimmed).collect();
    if list.is_empty() {
        return;
    }
    b.push_str(heading);
    b.push('\n');
    for item in &list {
        write_item(b, label, item);
    }
    b.push('\n');
}

/// The item with every field trimmed, or `None` when it has no id.
fn trimmed(item: &agentdef::Use) -> Option<agentdef::Use> {
    let id = item.id.trim();
    if id.is_empty() {
        return None;
    }
    Some(agentdef::Use {
        id: id.to_string(),
        name: item.name.trim().to_string(),
        description: item.description.trim().to_string(),
        board_id: item.board_id.trim().to_string(),
        board_name: item.board_name.trim().to_string(),
        space_id: item.space_id.trim().to_string(),
        space_key: item.space_key.trim().to_string(),
        space_name: item.space_name.trim().to_string(),
        parent_id: item.parent_id.trim().to_string(),
        parent_title: item.parent_title.trim().to_string(),
    })
}

fn write_item(b: &mut String, label: &str, item: &agentdef::Use) {
    let name = if item.name.is_empty() { &item.id } else { &item.name };
    w!(b, "- {}\n", name);
    write_place(b, item);
    if item.description.is_empty() {
        return;
    }
    if !label.is_empty() {
        w!(b, "  {}\n", label);
    }
    for line in item.description.split('\n') {
        w!(b, "  {}\n", line);
    }
}

/// The board, space, and parent page the item is limited to.
fn write_place(b: &mut String, item: &agentdef::Use) {
    let line = place_line(&item.board_name, &item.board_id);
    if !line.is_empty() {
        w!(b, "  Board: {}\n", line);
    }
    let mut line = place_line(&item.space_name, &item.space_key);
    if line.is_empty() {
        line = place_line("", &item.space_id);
    }
    if !line.is_empty() {
        w!(b, "  Space: {}\n", line);
    }
    let line = place_line(&item.parent_title, &item.parent_id);
    if !line.is_empty() {
        w!(b, "  Parent page: {}\n", line);
    }
}

/// "name (id)", or whichever of the two is set.
fn place_line(name: &str, id: &str) -> String {
    let name = name.trim();
    let id = id.trim();
    if name.is_empty() {
        return id.to_string();
    }
    if id.is_empty() || id == name {
        return name.to_string();
    }
    format!("{name} ({id})")
}
