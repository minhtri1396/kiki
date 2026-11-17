//! Pattern: State. The status of an agent's Orca terminal is one of four values, decided from what Orca reports.

use std::time::Duration;

use chrono::{DateTime, Utc};
use kiki_store::RunPhase;

/// A terminal that stopped printing counts as waiting only after it ran for this long.
const IDLE_GRACE: Duration = Duration::from_secs(20);

/// The status stored with a run. [`as_str`](RunStatus::as_str) is the text the web UI shows and the store keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunStatus {
    Running,
    /// Running, but the agent stopped printing: it is probably waiting for the user.
    Idle,
    Exited,
    Unknown,
}

impl RunStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Idle => "idle",
            RunStatus::Exited => "exited",
            RunStatus::Unknown => "unknown",
        }
    }

    /// What the board should show for this run, or `None` when Orca did not say enough to move the card.
    /// An agent that left a question is waiting for review whatever its terminal does.
    pub(crate) fn phase(self, has_question: bool) -> Option<RunPhase> {
        match self {
            _ if has_question => Some(RunPhase::Review),
            RunStatus::Idle | RunStatus::Exited => Some(RunPhase::Review),
            RunStatus::Running => Some(RunPhase::Doing),
            RunStatus::Unknown => None,
        }
    }
}

/// The status of a terminal from the Orca process status (`running`, `exited`, anything else is unknown).
/// A running terminal is idle only when it stopped printing and has run for longer than the grace time,
/// so a freshly started agent is not reported as waiting.
pub(crate) fn orca_run_status(process: &str, idle: bool, started_at: &str, now: DateTime<Utc>) -> RunStatus {
    match process {
        "exited" => RunStatus::Exited,
        "running" if idle && ran_longer_than_grace(started_at, now) => RunStatus::Idle,
        "running" => RunStatus::Running,
        _ => RunStatus::Unknown,
    }
}

fn ran_longer_than_grace(started_at: &str, now: DateTime<Utc>) -> bool {
    let Ok(started) = DateTime::parse_from_rfc3339(started_at) else {
        return false;
    };
    let elapsed = now.signed_duration_since(started.with_timezone(&Utc));
    elapsed >= chrono::Duration::from_std(IDLE_GRACE).unwrap_or_default()
}
