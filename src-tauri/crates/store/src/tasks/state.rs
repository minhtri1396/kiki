//! Pattern: State. A task's lifecycle is two small state machines, one per agent and one for the board card.
//!
//! [`AgentStatus`] moves `pending -> doing -> review -> completed` and owns every legal transition
//! ([`start`](AgentStatus::start), [`on_run_update`](AgentStatus::on_run_update),
//! [`resume`](AgentStatus::resume), [`approve`](AgentStatus::approve)). [`Column`] is derived from the agents'
//! statuses by [`Column::from_agents`]. Both serialize to the lowercase strings kept in SQLite and JSON.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::TaskError;

/// An agent's progress on one task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    /// The agent has not started this task.
    #[default]
    Pending,
    /// The agent is working on this task.
    Doing,
    /// The agent is waiting for review.
    Review,
    /// The agent finished this task. Only the user can get an agent here, by approving it.
    Completed,
}

/// The phase an Orca run reports: still working, or waiting for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    Doing,
    Review,
}

/// A board column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Column {
    /// The TODO column.
    #[default]
    Todo,
    /// The In Progress column (stored as `progress`).
    Progress,
    /// The Done column.
    Done,
}

/// A status or column text that is not one of the known values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownValue(pub String);

impl fmt::Display for UnknownValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown value {:?}", self.0)
    }
}

impl std::error::Error for UnknownValue {}

impl AgentStatus {
    /// The text stored in SQLite and sent as JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Pending => "pending",
            AgentStatus::Doing => "doing",
            AgentStatus::Review => "review",
            AgentStatus::Completed => "completed",
        }
    }

    /// Reads a stored status. Text that is not a status (an old or damaged row) counts as `pending`.
    pub(crate) fn from_stored(text: &str) -> AgentStatus {
        text.parse().unwrap_or_default()
    }

    /// True once the user approved the agent.
    pub fn is_completed(self) -> bool {
        self == AgentStatus::Completed
    }

    /// An Orca terminal was started for the agent: it is working.
    pub fn start(self) -> AgentStatus {
        AgentStatus::Doing
    }

    /// The latest Orca poll reported `phase`. A completed agent stays completed until the user approves.
    pub fn on_run_update(self, phase: RunPhase) -> AgentStatus {
        if self.is_completed() {
            return self;
        }
        match phase {
            RunPhase::Doing => AgentStatus::Doing,
            RunPhase::Review => AgentStatus::Review,
        }
    }

    /// The user sent a waiting agent back to work. Only `review` can resume.
    pub fn resume(self) -> Result<AgentStatus, TaskError> {
        match self {
            AgentStatus::Review => Ok(AgentStatus::Doing),
            _ => Err(TaskError::new("This agent is not waiting for review.")),
        }
    }

    /// The user approved a waiting agent. Only `review` can be approved.
    pub fn approve(self) -> Result<AgentStatus, TaskError> {
        match self {
            AgentStatus::Review => Ok(AgentStatus::Completed),
            _ => Err(TaskError::new(
                "Approve is available when the agent is waiting for review.",
            )),
        }
    }
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentStatus {
    type Err = UnknownValue;

    fn from_str(text: &str) -> Result<AgentStatus, UnknownValue> {
        match text {
            "pending" => Ok(AgentStatus::Pending),
            "doing" => Ok(AgentStatus::Doing),
            "review" => Ok(AgentStatus::Review),
            "completed" => Ok(AgentStatus::Completed),
            other => Err(UnknownValue(other.to_string())),
        }
    }
}

impl From<RunPhase> for AgentStatus {
    fn from(phase: RunPhase) -> AgentStatus {
        match phase {
            RunPhase::Doing => AgentStatus::Doing,
            RunPhase::Review => AgentStatus::Review,
        }
    }
}

impl RunPhase {
    pub fn as_str(self) -> &'static str {
        AgentStatus::from(self).as_str()
    }
}

impl Column {
    /// The text stored in SQLite and sent as JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Column::Todo => "todo",
            Column::Progress => "progress",
            Column::Done => "done",
        }
    }

    /// Reads a stored column. Text that is not a column counts as TODO.
    pub(crate) fn from_stored(text: &str) -> Column {
        text.parse().unwrap_or_default()
    }

    pub fn is_done(self) -> bool {
        self == Column::Done
    }

    /// The column a task belongs in. A task that has not started stays where it is; a started task is
    /// Done when it has agents and every one is completed, and In Progress otherwise.
    pub fn from_agents(current: Column, started: bool, statuses: &[AgentStatus]) -> Column {
        if !started {
            return current;
        }
        if !statuses.is_empty() && statuses.iter().all(|status| status.is_completed()) {
            Column::Done
        } else {
            Column::Progress
        }
    }
}

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Column {
    type Err = UnknownValue;

    fn from_str(text: &str) -> Result<Column, UnknownValue> {
        match text {
            "todo" => Ok(Column::Todo),
            "progress" => Ok(Column::Progress),
            "done" => Ok(Column::Done),
            other => Err(UnknownValue(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [AgentStatus; 4] = [
        AgentStatus::Pending,
        AgentStatus::Doing,
        AgentStatus::Review,
        AgentStatus::Completed,
    ];

    #[test]
    fn strings_match_the_stored_text() {
        for (status, text) in ALL.iter().zip(["pending", "doing", "review", "completed"]) {
            assert_eq!(status.as_str(), text);
            assert_eq!(status.to_string(), text);
            assert_eq!(text.parse::<AgentStatus>().unwrap(), *status);
            assert_eq!(serde_json::to_string(status).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<AgentStatus>(&format!("\"{text}\"")).unwrap(),
                *status
            );
        }
        for (column, text) in [
            (Column::Todo, "todo"),
            (Column::Progress, "progress"),
            (Column::Done, "done"),
        ] {
            assert_eq!(column.as_str(), text);
            assert_eq!(text.parse::<Column>().unwrap(), column);
            assert_eq!(serde_json::to_string(&column).unwrap(), format!("\"{text}\""));
        }
        assert!("Doing".parse::<AgentStatus>().is_err());
        assert!("in progress".parse::<Column>().is_err());
        assert_eq!(AgentStatus::from_stored("garbage"), AgentStatus::Pending);
        assert_eq!(Column::from_stored(""), Column::Todo);
    }

    #[test]
    fn run_updates_move_between_doing_and_review_but_never_leave_completed() {
        for phase in [RunPhase::Doing, RunPhase::Review] {
            assert_eq!(AgentStatus::Completed.on_run_update(phase), AgentStatus::Completed);
            for status in [AgentStatus::Pending, AgentStatus::Doing, AgentStatus::Review] {
                assert_eq!(status.on_run_update(phase), AgentStatus::from(phase));
            }
        }
        assert_eq!(RunPhase::Review.as_str(), "review");
    }

    #[test]
    fn start_always_means_doing() {
        for status in ALL {
            assert_eq!(status.start(), AgentStatus::Doing);
        }
    }

    #[test]
    fn only_a_waiting_agent_can_resume_or_be_approved() {
        assert_eq!(AgentStatus::Review.resume().unwrap(), AgentStatus::Doing);
        assert_eq!(AgentStatus::Review.approve().unwrap(), AgentStatus::Completed);
        for status in [AgentStatus::Pending, AgentStatus::Doing, AgentStatus::Completed] {
            assert_eq!(
                status.resume().unwrap_err().message,
                "This agent is not waiting for review."
            );
            assert_eq!(
                status.approve().unwrap_err().message,
                "Approve is available when the agent is waiting for review."
            );
        }
    }

    #[test]
    fn column_follows_the_agents_of_a_started_task() {
        use AgentStatus::*;
        assert_eq!(
            Column::from_agents(Column::Todo, false, &[Completed]),
            Column::Todo,
            "not started keeps its column"
        );
        assert_eq!(Column::from_agents(Column::Done, false, &[Doing]), Column::Done);
        assert_eq!(
            Column::from_agents(Column::Todo, true, &[]),
            Column::Progress,
            "no agents is never done"
        );
        assert_eq!(
            Column::from_agents(Column::Todo, true, &[Doing, Completed]),
            Column::Progress
        );
        assert_eq!(
            Column::from_agents(Column::Progress, true, &[Completed, Completed]),
            Column::Done
        );
        assert_eq!(
            Column::from_agents(Column::Done, true, &[Completed, Review]),
            Column::Progress
        );
    }
}
