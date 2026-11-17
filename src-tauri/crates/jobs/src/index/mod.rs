//! Pattern: Strategy + Factory. One `IndexTool` per index (Obsidian, CodeGraph) owns its command, its setup steps and
//! its status check; `for_name` is the factory that turns the saved index name into the strategy.

mod codegraph;
mod obsidian;

use crate::env::Env;
use crate::model::Check;
use anyhow::Result;
use kiki_util::Ctx;

/// The repository a setup request is for.
pub(crate) struct Setup<'a> {
    /// The app root (the directory CodeGraph runs in).
    pub(crate) root: &'a str,
    /// The repository kind; already validated.
    pub(crate) kind: &'a str,
    /// The repository folder; already known to be non-blank.
    pub(crate) folder: &'a str,
}

/// What a tool wants launched in the background.
#[derive(Clone, Debug, Default)]
pub(crate) struct Launch {
    /// The working directory.
    pub(crate) dir: String,
    /// The command and its arguments; the first item is the command name.
    pub(crate) args: Vec<String>,
    /// Text shown first in the job log.
    pub(crate) note: String,
}

/// One index tool.
pub(crate) trait IndexTool: Sync {
    /// The saved index name and the job kind: `obsidian` or `codegraph`.
    fn name(&self) -> &'static str;

    /// The directory the tool keeps inside a repository (`.obsidian`, `.codegraph`).
    fn dir_name(&self) -> &'static str;

    /// The setup steps that run before the background command starts.
    fn prepare(&self, env: &Env, req: &Setup<'_>) -> Result<Launch>;

    /// Runs the tool's status command for a repository and reports whether the index is ready.
    /// `folder` is trimmed and may be empty.
    fn check(&self, env: &Env, ctx: &Ctx, root: &str, folder: &str) -> Check;

    /// Hook of the background run: a problem the finished command's log reveals even though it exited cleanly.
    fn judge_success(&self, _log: &str) -> Option<String> {
        None
    }

    /// Hook: the error to report when the job could not be launched.
    fn start_error(&self, _launch: &Launch, err: anyhow::Error) -> anyhow::Error {
        err
    }
}

/// The Obsidian strategy.
pub(crate) fn obsidian() -> &'static dyn IndexTool {
    &obsidian::Obsidian
}

/// The CodeGraph strategy.
pub(crate) fn codegraph() -> &'static dyn IndexTool {
    &codegraph::Codegraph
}

/// The factory: the strategy for a saved index name, or `None` for no index or an unknown one.
pub(crate) fn for_name(name: &str) -> Option<&'static dyn IndexTool> {
    match name {
        "obsidian" => Some(obsidian()),
        "codegraph" => Some(codegraph()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_maps_names_to_tools() {
        assert_eq!(for_name("obsidian").unwrap().name(), "obsidian");
        assert_eq!(for_name("obsidian").unwrap().dir_name(), ".obsidian");
        assert_eq!(for_name("codegraph").unwrap().dir_name(), ".codegraph");
        assert!(for_name("").is_none() && for_name("git").is_none());
    }

    #[test]
    fn only_obsidian_judges_the_log() {
        assert!(for_name("codegraph")
            .unwrap()
            .judge_success("Vault not found.")
            .is_none());
        let problem = for_name("obsidian").unwrap().judge_success("x\nVault not found.\n");
        assert_eq!(problem.as_deref(), Some("Vault not found."));
        assert!(for_name("obsidian").unwrap().judge_success("fine").is_none());
    }
}
