//! Tests for the agent folder: starter catalog, saving and parsing.

use crate::*;

use std::path::Path;

mod catalog;
mod parse;
mod save;

pub(super) fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

pub(super) fn s(p: &Path) -> String {
    p.to_str().unwrap().to_string()
}

pub(super) fn agents_dir(td: &tempfile::TempDir) -> String {
    s(&td.path().join("agents"))
}

pub(super) fn find_ok(dir: &str, id: &str) -> File {
    find(dir, id).unwrap().unwrap_or_else(|| panic!("agent {id} not found"))
}
