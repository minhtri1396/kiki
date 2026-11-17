//! Board task types and the typed errors callers branch on.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::state::{AgentStatus, Column};
use crate::runs::TaskRun;

/// ErrTaskNotFound is returned when a task id is not in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrTaskNotFound;

impl fmt::Display for ErrTaskNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("task was not found")
    }
}

impl std::error::Error for ErrTaskNotFound {}

/// Reports whether `err` is [`ErrTaskNotFound`].
pub fn is_task_not_found(err: &anyhow::Error) -> bool {
    err.downcast_ref::<ErrTaskNotFound>().is_some()
}

/// TaskError is a task the user can fix.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskError {
    pub message: String,
}

impl TaskError {
    pub fn new(message: impl Into<String>) -> TaskError {
        TaskError {
            message: message.into(),
        }
    }
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TaskError {}

/// Task is one board card.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub description: String,
    pub column: Column,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "doneAt")]
    pub done_at: String,
    #[serde(rename = "branchSlug")]
    pub branch_slug: String,
    #[serde(rename = "branchFor")]
    pub branch_for: String,
    pub started: bool,
    #[serde(rename = "parentId")]
    pub parent_id: String,
    #[serde(rename = "groupId")]
    pub group_id: String,
    #[serde(rename = "epicKey")]
    pub epic_key: String,
    #[serde(rename = "epicBoardId")]
    pub epic_board_id: String,
    #[serde(rename = "epicBoardName")]
    pub epic_board_name: String,
    #[serde(rename = "epicOwner")]
    pub epic_owner: bool,
    pub agents: Vec<TaskAgent>,
    pub runs: Vec<TaskRun>,
}

/// TaskAgent is one agent's status on a task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskAgent {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub status: AgentStatus,
}

/// An agent picked for a task, as the user typed it. `status` is checked when the task is saved
/// (empty means pending), and the stored status of an agent already on the task is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentInput {
    pub agent_id: String,
    pub status: String,
}

impl AgentInput {
    pub fn new(agent_id: impl Into<String>, status: impl Into<String>) -> AgentInput {
        AgentInput {
            agent_id: agent_id.into(),
            status: status.into(),
        }
    }
}

impl From<&TaskAgent> for AgentInput {
    fn from(agent: &TaskAgent) -> AgentInput {
        AgentInput {
            agent_id: agent.agent_id.clone(),
            status: agent.status.to_string(),
        }
    }
}

/// TaskInput is the editable part of a task. `column` is the text the user chose (empty means TODO).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskInput {
    pub name: String,
    pub description: String,
    pub column: String,
    pub parent_id: String,
    pub group_id: String,
    pub epic_key: String,
    pub epic_board_id: String,
    pub epic_board_name: String,
    pub epic_owner: bool,
    pub agents: Vec<AgentInput>,
}

/// A task that passed validation and is ready to store. Its group link is resolved from `group_id`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TaskDraft {
    pub name: String,
    pub description: String,
    pub column: Column,
    pub parent_id: String,
    pub group_id: String,
    pub epic_key: String,
    pub epic_board_id: String,
    pub epic_board_name: String,
    pub epic_owner: bool,
    /// Agent ids in the order the user picked them, without duplicates.
    pub agents: Vec<String>,
}
