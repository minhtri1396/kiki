//! The data the manager hands out: jobs, status checks, repository targets and reports.

use kiki_util::Ctx;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Where a setup job is in its life. Serialized as the lowercase word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// The command is still running.
    #[default]
    Running,
    /// The command finished successfully.
    Ok,
    /// The command failed, timed out, or reported a problem in its output.
    Error,
    /// Someone cancelled the job.
    Cancelled,
}

impl JobStatus {
    /// The serialized word.
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Ok => "ok",
            JobStatus::Error => "error",
            JobStatus::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Job is one setup command.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Job {
    #[serde(rename = "id")]
    pub id: String,
    /// The index tool that runs: `obsidian` or `codegraph`.
    #[serde(rename = "kind")]
    pub kind: String,
    /// The repository kind the job works on.
    #[serde(rename = "target")]
    pub target: String,
    #[serde(rename = "status")]
    pub status: JobStatus,
    #[serde(rename = "command")]
    pub command: String,
    #[serde(rename = "log")]
    pub log: String,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "finishedAt", default, skip_serializing_if = "String::is_empty")]
    pub finished_at: String,
    #[serde(rename = "error", default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(skip)]
    pub(crate) cancel: Option<Ctx>,
}

/// Check is the result of one status command.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    #[serde(rename = "ready")]
    pub ready: bool,
    #[serde(rename = "dirExists")]
    pub dir_exists: bool,
    #[serde(rename = "summary")]
    pub summary: String,
    #[serde(rename = "checkedAt", default, skip_serializing_if = "String::is_empty")]
    pub checked_at: String,
}

impl Check {
    /// A not-ready check that only carries a message, stamped now.
    pub(crate) fn not_ready(summary: impl Into<String>) -> Check {
        Check {
            summary: summary.into(),
            checked_at: kiki_util::now_rfc3339(),
            ..Default::default()
        }
    }
}

/// Target is one repository and the index saved for it.
/// `index` is obsidian, codegraph, or empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Target {
    pub kind: String,
    pub folder: String,
    pub index: String,
}

/// Report is the last status check, keyed by repository kind.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    #[serde(rename = "repos")]
    pub repos: BTreeMap<String, Check>,
}
