//! Helpers shared by the unit tests.

use std::path::Path;

use tempfile::TempDir;

pub(crate) fn tmp() -> TempDir {
    tempfile::tempdir().unwrap()
}

pub(crate) fn p(dir: &TempDir) -> String {
    dir.path().to_string_lossy().into_owned()
}

pub(crate) fn join(base: &str, rel: &str) -> String {
    Path::new(base).join(rel).to_string_lossy().into_owned()
}

pub(crate) fn sv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}
