//! The repository and index rows of the state document.

use serde_json::{json, Value};

use kiki_console as console;
use kiki_jobs as jobs;

use super::index_tool::{present_index_tools, IndexTool};
use super::support::is_dir;

/// One row per repository: folder, services, index, and what git says about it.
pub(crate) fn repo_rows(snap: &console::Snapshot) -> Vec<Value> {
    snap.repos
        .iter()
        .map(|repo| {
            let divided = repo.uses_services();
            let services = if divided { repo.services.clone() } else { Vec::new() };
            let available = if divided {
                console::scan_services(&repo.folder).unwrap_or_default()
            } else {
                Vec::new()
            };
            let exists = !repo.folder.is_empty() && is_dir(&repo.folder);
            let empty = !repo.folder.is_empty() && crate::clone::folder_empty(&repo.folder).unwrap_or(false);
            let service_guides: Vec<Value> = if divided {
                repo.service_guides
                    .iter()
                    .map(|row| {
                        json!({
                            "service": row.service,
                            "shared": row.shared,
                            "guides": if row.shared { Vec::<String>::new() } else { row.guides.clone() },
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            json!({
                "kind": repo.kind,
                "name": repo.title(),
                "folder": repo.folder,
                "divided": divided,
                "services": services,
                "guides": repo.guides,
                "serviceGuides": service_guides,
                "available": available,
                "exists": exists,
                "empty": empty,
                "index": repo.index,
                "sparse": repo.sparse,
                "git": crate::clone::git_origin(&repo.folder),
            })
        })
        .collect()
}

/// One row per repository: which index tool it uses, whether that is ready, and the command that sets it up.
pub(crate) fn index_rows(snap: &console::Snapshot, report: &jobs::Report) -> Vec<Value> {
    snap.repos
        .iter()
        .map(|repo| {
            let check = report.repos.get(&repo.kind).cloned().unwrap_or_default();
            let folder = repo.folder.trim().to_string();
            let tool = IndexTool::parse(&repo.index);
            let dir_exists = tool.is_some_and(|tool| tool.present_in(&folder));
            let command = tool.map(|tool| tool.command(&folder)).unwrap_or_default();
            let checked = !check.checked_at.is_empty();
            let ready = checked && !repo.index.is_empty() && check.ready && dir_exists;
            json!({
                "kind": repo.kind,
                "index": repo.index,
                "dirExists": dir_exists,
                "present": present_index_tools(&folder),
                "ready": ready,
                "checked": checked,
                "summary": check.summary,
                "command": command,
                "folder": folder,
            })
        })
        .collect()
}
