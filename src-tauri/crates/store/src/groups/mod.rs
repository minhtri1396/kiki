//! Board groups. Creating one creates its Jira epic. `Store` methods (facade) over [`repo::GroupRepo`].

mod repo;

use std::fmt;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub(crate) use repo::GroupRepo;

use crate::store::Store;
use crate::tasks::{epic_key_matches, TaskError};

/// ErrGroupNotFound is returned when a group id is not in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrGroupNotFound;

impl fmt::Display for ErrGroupNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("group was not found")
    }
}

impl std::error::Error for ErrGroupNotFound {}

/// Reports whether `err` is [`ErrGroupNotFound`].
pub fn is_group_not_found(err: &anyhow::Error) -> bool {
    err.downcast_ref::<ErrGroupNotFound>().is_some()
}

/// Group is a board group. Creating one creates its Jira epic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "epicKey")]
    pub epic_key: String,
    #[serde(rename = "epicBoardId")]
    pub epic_board_id: String,
    #[serde(rename = "epicBoardName")]
    pub epic_board_name: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub deleted: bool,
}

/// GroupInput is the stored part of a group. The epic is already created.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupInput {
    pub name: String,
    pub description: String,
    pub epic_key: String,
    pub epic_board_id: String,
    pub epic_board_name: String,
}

impl Store {
    /// Groups returns every group, newest first.
    pub fn groups(&self) -> Result<Vec<Group>> {
        self.with_db(|db| GroupRepo::new(db).list())
    }

    /// Group returns one group.
    pub fn group(&self, id: &str) -> Result<Group> {
        self.with_db(|db| GroupRepo::new(db).get(id))
    }

    /// CreateGroup stores a group that already has a Jira epic.
    pub fn create_group(&self, input: GroupInput) -> Result<Group> {
        let input = normalize_group(input)?;
        self.with_db(|db| GroupRepo::new(db).create(&input))
    }

    /// DeleteGroup marks a group deleted. Tasks that already use it keep that link.
    pub fn delete_group(&self, id: &str) -> Result<()> {
        let id = id.trim();
        if id.is_empty() {
            return Err(ErrGroupNotFound.into());
        }
        self.with_db(|db| GroupRepo::new(db).mark_deleted(id))
    }
}

fn normalize_group(mut input: GroupInput) -> Result<GroupInput> {
    input.name = input.name.trim().to_string();
    input.description = input.description.trim().to_string();
    input.epic_key = input.epic_key.trim().to_uppercase();
    input.epic_board_id = input.epic_board_id.trim().to_string();
    input.epic_board_name = input.epic_board_name.trim().to_string();
    if input.name.is_empty() {
        return Err(TaskError::new("Enter a group name.").into());
    }
    if input.name.len() > 200 {
        return Err(TaskError::new("Group name is longer than 200 characters.").into());
    }
    if input.description.len() > 4000 {
        return Err(TaskError::new("Description is longer than 4000 characters.").into());
    }
    if input.epic_key.is_empty() {
        input.epic_board_id = String::new();
        input.epic_board_name = String::new();
        return Ok(input);
    }
    if !epic_key_matches(&input.epic_key) {
        return Err(TaskError::new("The Jira epic key is not valid.").into());
    }
    if input.epic_board_id.is_empty() {
        return Err(TaskError::new("The Jira epic needs a board.").into());
    }
    Ok(input)
}
