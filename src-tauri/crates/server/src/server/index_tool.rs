//! Pattern: State. The tool that indexes a repository is a closed set; `IndexTool` carries what differs per tool
//! (its id, the folder it leaves in the repository, the command shown in Settings) instead of string compares.

use kiki_jobs as jobs;
use kiki_toolchain as toolchain;
use kiki_util::path;

use super::support::is_dir;

/// An index tool a repository can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexTool {
    Obsidian,
    Codegraph,
}

impl IndexTool {
    pub(crate) const ALL: [IndexTool; 2] = [IndexTool::Obsidian, IndexTool::Codegraph];

    /// The id saved in the setup and sent to the web UI.
    pub(crate) fn id(self) -> &'static str {
        match self {
            IndexTool::Obsidian => "obsidian",
            IndexTool::Codegraph => "codegraph",
        }
    }

    pub(crate) fn parse(id: &str) -> Option<IndexTool> {
        IndexTool::ALL.into_iter().find(|tool| tool.id() == id)
    }

    /// The folder the tool keeps inside the repository.
    pub(crate) fn dir(self) -> &'static str {
        match self {
            IndexTool::Obsidian => ".obsidian",
            IndexTool::Codegraph => ".codegraph",
        }
    }

    /// Whether `folder` already holds this tool's folder.
    pub(crate) fn present_in(self, folder: &str) -> bool {
        let folder = folder.trim();
        !folder.is_empty() && is_dir(&path::join(&[folder, self.dir()]))
    }

    /// The command that sets the index up (or checks it), as shown in Settings.
    pub(crate) fn command(self, folder: &str) -> String {
        match (self, folder.is_empty()) {
            (IndexTool::Obsidian, true) => "obsidian vault info=path".to_string(),
            (IndexTool::Obsidian, false) => jobs::format_args(&toolchain::obsidian_status(folder)),
            (IndexTool::Codegraph, true) => "codegraph init <folder>".to_string(),
            (IndexTool::Codegraph, false) => jobs::format_args(&toolchain::codegraph_init(folder)),
        }
    }
}

/// The index tools whose folder is in `folder`.
pub(crate) fn present_index_tools(folder: &str) -> Vec<&'static str> {
    IndexTool::ALL
        .into_iter()
        .filter(|tool| tool.present_in(folder))
        .map(IndexTool::id)
        .collect()
}

/// The only index tool in an existing `folder`; `None` when there is none or both.
pub(crate) fn sole_index_tool(folder: &str) -> Option<IndexTool> {
    let folder = folder.trim();
    if folder.is_empty() || !is_dir(folder) {
        return None;
    }
    match (
        IndexTool::Obsidian.present_in(folder),
        IndexTool::Codegraph.present_in(folder),
    ) {
        (true, false) => Some(IndexTool::Obsidian),
        (false, true) => Some(IndexTool::Codegraph),
        _ => None,
    }
}
