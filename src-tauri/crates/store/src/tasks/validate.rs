//! Validation of what the user typed for a task. Messages are shown in the UI as written.

use std::collections::HashSet;

use anyhow::Result;

use super::model::{TaskDraft, TaskError, TaskInput};
use super::state::{AgentStatus, Column};

/// `^[A-Z][A-Z0-9]{1,9}-[0-9]+$`
pub(crate) fn epic_key_matches(key: &str) -> bool {
    let Some((head, tail)) = key.split_once('-') else {
        return false;
    };
    let mut chars = head.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    let rest: Vec<char> = chars.collect();
    if rest.is_empty() || rest.len() > 9 || !rest.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return false;
    }
    !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit())
}

fn valid_agent_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 80 {
        return false;
    }
    id.chars().all(|r| r.is_ascii_alphanumeric() || r == '_' || r == '-')
}

/// Reads the column the user chose; empty means TODO.
fn parse_column(raw: &str) -> Result<Column, TaskError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Column::Todo);
    }
    raw.parse()
        .map_err(|_| TaskError::new("Choose TODO, In Progress, or Done."))
}

/// Trims and validates the editable fields. The agents' statuses are checked but not kept: a task takes
/// its agents' stored statuses (or pending for a new agent).
pub(crate) fn normalize_task(input: TaskInput) -> Result<TaskDraft> {
    let name = input.name.trim().to_string();
    let description = input.description.trim().to_string();
    let column = parse_column(&input.column)?;
    if name.is_empty() {
        return Err(TaskError::new("Enter a task name.").into());
    }
    if name.len() > 200 {
        return Err(TaskError::new("Task name is longer than 200 characters.").into());
    }
    if description.len() > 4000 {
        return Err(TaskError::new("Description is longer than 4000 characters.").into());
    }
    if input.agents.len() > 100 {
        return Err(TaskError::new("A task can use at most 100 agents.").into());
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut agents: Vec<String> = Vec::with_capacity(input.agents.len());
    for agent in &input.agents {
        let id = agent.agent_id.trim().to_string();
        let status = agent.status.trim();
        if id.is_empty() {
            return Err(TaskError::new("Choose an agent.").into());
        }
        if !valid_agent_id(&id) {
            return Err(TaskError::new(format!("{id} is not an agent in this app.")).into());
        }
        if !status.is_empty() && status.parse::<AgentStatus>().is_err() {
            return Err(TaskError::new("Agent status is not valid.").into());
        }
        if seen.insert(id.clone()) {
            agents.push(id);
        }
    }
    Ok(TaskDraft {
        name,
        description,
        column,
        parent_id: input.parent_id,
        group_id: input.group_id,
        epic_key: input.epic_key,
        epic_board_id: input.epic_board_id,
        epic_board_name: input.epic_board_name,
        epic_owner: input.epic_owner,
        agents,
    })
}
