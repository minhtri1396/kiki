//! Required inputs from the agent document. The launch prompt tells the agent to ask
//! for each one the user left out of the task.

/// Asks for every input the agent document marks `Required:`.
pub(super) fn write(b: &mut String, body: &str) {
    let names = required_names(body);
    if names.is_empty() {
        return;
    }
    b.push('\n');
    b.push_str("Required input. These inputs are required: ");
    b.push_str(&names.join(", "));
    b.push_str(
        ". If the user did not give one of them when they asked you to do a task, \
         ask the user for it before you continue. Do not guess. \
         Write each missing input as its own Q in the questions block, with A: text.\n",
    );
}

/// The input names marked required, in the order they appear.
fn required_names(body: &str) -> Vec<String> {
    let mut current = String::new();
    let mut names = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if let Some(name) = bold_item(line) {
            current = name;
            continue;
        }
        if !line.starts_with("Required:") || current.is_empty() {
            continue;
        }
        names.push(std::mem::take(&mut current));
    }
    names
}

/// The name of a list item whose line is only `- **Name**`.
fn bold_item(line: &str) -> Option<String> {
    let rest = line.strip_prefix("- **")?;
    let name = rest.strip_suffix("**")?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}
