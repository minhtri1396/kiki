//! Status checks: run each repository's index tool (in parallel) or just look for its folder, and keep the report.

use crate::index::{for_name, IndexTool};
use crate::manager::Manager;
use crate::model::{Check, Report, Target};
use crate::text::{clean_output, first_line, truncate_bytes};
use anyhow::{anyhow, Result};
use kiki_util::{path, Ctx};
use std::sync::Mutex;

impl Manager {
    /// Check runs the status command for the index saved on each repository.
    pub fn check(&self, ctx: &Ctx, root: &str, targets: &[Target]) -> Result<Report> {
        let report = Mutex::new(Report::default());
        std::thread::scope(|scope| {
            for target in targets {
                let report = &report;
                scope.spawn(move || {
                    let check = self.check_target(ctx, root, target);
                    report
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .repos
                        .insert(target.kind.clone(), check);
                });
            }
        });
        let report = report.into_inner().unwrap_or_else(|e| e.into_inner());
        self.lock().last = report.clone();
        Ok(report)
    }

    /// RefreshFolders reports whether each repository using index still has its
    /// .obsidian or .codegraph folder. Other results stay as they were.
    pub fn refresh_folders(&self, targets: &[Target], index: &str) -> Result<Report> {
        let tool = for_name(index).ok_or_else(|| anyhow!("unknown index"))?;
        let mut st = self.lock();
        for target in targets.iter().filter(|target| target.index == index) {
            st.last.repos.insert(target.kind.clone(), folder_check(tool, target));
        }
        Ok(st.last.clone())
    }

    /// CheckOne runs the status command for one repository and keeps the other results.
    pub fn check_one(&self, ctx: &Ctx, root: &str, targets: &[Target], kind: &str) -> Result<Report> {
        let target = match targets.iter().find(|item| item.kind == kind) {
            Some(t) if kiki_toolchain::repo_id(kind) => t,
            _ => return Err(anyhow!("unknown repository")),
        };
        let check = self.check_target(ctx, root, target);
        let mut st = self.lock();
        st.last.repos.insert(kind.to_string(), check);
        Ok(st.last.clone())
    }

    fn check_target(&self, ctx: &Ctx, root: &str, target: &Target) -> Check {
        if target.index.is_empty() {
            return Check::not_ready("No index is set for this repository");
        }
        match for_name(&target.index) {
            Some(tool) => tool.check(&self.env, ctx, root, target.folder.trim()),
            None => Check::not_ready("unknown index"),
        }
    }
}

/// Whether the tool's folder exists in the repository, without running the tool.
fn folder_check(tool: &dyn IndexTool, target: &Target) -> Check {
    let name = tool.dir_name();
    let folder = target.folder.trim();
    let (exists, summary) = if folder.is_empty() {
        (false, "folder is not set".to_string())
    } else if is_dir(&path::join(&[folder, name])) {
        (true, format!("{name} exists"))
    } else {
        (false, format!("{name} is missing"))
    };
    Check {
        ready: exists,
        dir_exists: exists,
        summary,
        checked_at: kiki_util::now_rfc3339(),
    }
}

pub(crate) fn is_dir(path: &str) -> bool {
    std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// `p` made absolute against the current directory; `None` when that cannot be read.
pub(crate) fn abs_path(p: &str) -> Option<String> {
    if path::is_abs(p) {
        return Some(path::clean(p));
    }
    let cwd = std::env::current_dir().ok()?;
    Some(path::join(&[&cwd.to_string_lossy(), p]))
}

/// The check for a status command that is not installed.
pub(crate) fn command_missing(dir_exists: bool, command: &str) -> Check {
    Check {
        dir_exists,
        summary: format!("command {command} was not found on PATH"),
        checked_at: kiki_util::now_rfc3339(),
        ..Default::default()
    }
}

/// Whether `text` names `folder`, either as given or with symlinks resolved.
pub(crate) fn path_mentioned(text: &str, folder: &str) -> bool {
    let folder = path::clean(folder);
    if text.contains(&folder) {
        return true;
    }
    let Ok(resolved) = std::fs::canonicalize(&folder) else {
        return false;
    };
    let resolved = path::clean(&resolved.to_string_lossy());
    resolved != folder && text.contains(&resolved)
}

/// Turns a status command's output into a check. `phrase` is the lowercase text that means setup is missing.
pub(crate) fn evaluate(dir_exists: bool, output: &str, run_err: Option<&str>, phrase: &str) -> Check {
    let text = clean_output(output);
    let missing = text.to_lowercase().contains(phrase);
    let ready = dir_exists && run_err.is_none() && !missing;
    let mut summary = first_line(&text);
    if missing {
        summary = format!("The command reports that setup is missing ({phrase})");
    } else if summary.is_empty() {
        if let Some(err) = run_err {
            summary = err.to_string();
        } else if !dir_exists {
            summary = "Index folder is missing".to_string();
        } else if ready {
            summary = "Ready".to_string();
        }
    }
    Check {
        ready,
        dir_exists,
        summary: truncate_bytes(summary, 400),
        checked_at: kiki_util::now_rfc3339(),
    }
}
