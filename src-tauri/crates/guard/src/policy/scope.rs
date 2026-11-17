//! Repository checks of a policy: which folders, Nx projects, and read-only repositories an action may touch.

use super::model::{Decision, Policy};
use super::paths::{expand_path, inside_any};

impl Policy {
    /// Blocks work outside the repositories set for this agent. With no paths, the working folder is checked.
    pub(super) fn veto_paths(&self, cwd: &str, home: &str, paths: &[String]) -> Option<Decision> {
        if paths.is_empty() {
            if inside_any(&self.roots, cwd) {
                return None;
            }
            return Some(Decision::block(format!(
                "Guardrail: this agent cannot work in {cwd}. It is outside the repositories set for this agent."
            )));
        }
        let outside = local_paths(paths, cwd, home).find(|full| !inside_any(&self.roots, full))?;
        Some(Decision::block(format!(
            "Guardrail: this agent cannot use {outside}. It is outside the repositories set for this agent."
        )))
    }

    /// Blocks the working folder or a path inside an Nx project the repository did not select.
    pub(super) fn veto_services(&self, cwd: &str, home: &str, paths: &[String]) -> Option<Decision> {
        if self.services.is_empty() {
            return None;
        }
        self.veto_service_path(cwd)
            .or_else(|| local_paths(paths, cwd, home).find_map(|full| self.veto_service_path(&full)))
    }

    /// Blocks a write in the working folder or to a path inside a read-only repository.
    pub(super) fn veto_read_only(&self, cwd: &str, home: &str, paths: &[String]) -> Option<Decision> {
        if self.read_only.is_empty() {
            return None;
        }
        let readonly = |full: &str| {
            Decision::block(format!(
                "Guardrail: this agent can only read {full}. That repository is readonly."
            ))
        };
        if inside_any(&self.read_only, cwd) {
            return Some(readonly(cwd));
        }
        local_paths(paths, cwd, home)
            .find(|full| inside_any(&self.read_only, full))
            .map(|full| readonly(&full))
    }
}

/// The absolute form of every path that is a file path (not a URL) and can be made absolute.
fn local_paths<'a>(paths: &'a [String], cwd: &'a str, home: &'a str) -> impl Iterator<Item = String> + 'a {
    paths
        .iter()
        .filter(|path| !path.contains("://"))
        .map(move |path| expand_path(path, cwd, home))
        .filter(|full| !full.is_empty() && full != ".")
}
