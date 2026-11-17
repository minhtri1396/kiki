//! Board tasks and the agents working on them: the `Store` methods (facade) over [`repo::TaskRepo`].
//!
//! * `model`: task types and typed errors. * `state`: the agent status and column state machines.
//! * `validate`: input checks. * `repo`: SQL. * `schema`: tables and migrations. * `ids`: ULIDs.

mod ids;
mod model;
mod repo;
pub(crate) mod schema;
mod state;
mod validate;

use anyhow::Result;
use rusqlite::Connection;

pub(crate) use ids::new_task_id;
pub use model::{is_task_not_found, AgentInput, ErrTaskNotFound, Task, TaskAgent, TaskError, TaskInput};
pub(crate) use repo::TaskRepo;
pub use state::{AgentStatus, Column, RunPhase, UnknownValue};
pub(crate) use validate::epic_key_matches;

use crate::groups::{is_group_not_found, GroupRepo};
use crate::store::{now, Store};
use model::TaskDraft;
use validate::normalize_task;

#[cfg(test)]
pub(crate) use ids::encode_ulid;

impl Store {
    /// Tasks returns every task, newest activity first.
    pub fn tasks(&self) -> Result<Vec<Task>> {
        self.with_db(|db| TaskRepo::new(db).list())
    }

    /// Task returns one task.
    pub fn task(&self, id: &str) -> Result<Task> {
        self.with_db(|db| TaskRepo::new(db).get(id))
    }

    /// CreateTask stores a new task. It always starts in the TODO column.
    pub fn create_task(&self, input: TaskInput) -> Result<Task> {
        let mut draft = normalize_task(input)?;
        draft.column = Column::Todo;
        self.with_db(|db| {
            let draft = resolve_group_link(db, draft)?;
            let stamp = now();
            TaskRepo::new(db).insert(&draft, &stamp)
        })
    }

    /// UpdateTask replaces the editable fields of one task. The column and DoneAt stay as they are.
    pub fn update_task(&self, id: &str, input: TaskInput) -> Result<Task> {
        let mut draft = normalize_task(input)?;
        self.with_db(|db| {
            let tasks = TaskRepo::new(db);
            let prev = tasks.get(id)?;
            if prev.started {
                return Err(TaskError::new("This task has already started.").into());
            }
            draft.column = prev.column;
            let draft = resolve_group_link(db, draft)?;
            tasks.update(id, &draft, &now(), &prev.done_at)
        })
    }

    /// DeleteTask removes one task and its agents.
    pub fn delete_task(&self, id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(ErrTaskNotFound.into());
        }
        self.with_db(|db| TaskRepo::new(db).delete(id))
    }

    /// SetTaskBranch stores the English branch slug made from source, the task name it came from.
    pub fn set_task_branch(&self, id: &str, slug: &str, source: &str) -> Result<Task> {
        let slug = slug.trim();
        let source = source.trim();
        if id.is_empty() {
            return Err(ErrTaskNotFound.into());
        }
        if slug.is_empty() {
            return Err(TaskError::new("The branch name could not be made from this task name.").into());
        }
        if slug.len() > 80 {
            return Err(TaskError::new("The branch name is too long.").into());
        }
        self.with_db(|db| {
            let tasks = TaskRepo::new(db);
            tasks.set_branch(id, slug, source)?;
            tasks.get(id)
        })
    }
}

/// Points the task at its group's Jira epic. A task without a group has no epic link and no parent.
fn resolve_group_link(db: &Connection, mut draft: TaskDraft) -> Result<TaskDraft> {
    draft.group_id = draft.group_id.trim().to_string();
    draft.parent_id = String::new();
    draft.epic_key = String::new();
    draft.epic_board_id = String::new();
    draft.epic_board_name = String::new();
    draft.epic_owner = false;
    if draft.group_id.is_empty() {
        return Ok(draft);
    }
    let group = match GroupRepo::new(db).get(&draft.group_id) {
        Ok(group) => group,
        Err(err) if is_group_not_found(&err) => return Err(TaskError::new("That group was not found.").into()),
        Err(err) => return Err(err),
    };
    draft.epic_key = group.epic_key;
    draft.epic_board_id = group.epic_board_id;
    draft.epic_board_name = group.epic_board_name;
    Ok(draft)
}
