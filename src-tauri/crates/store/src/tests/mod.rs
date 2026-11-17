//! Store tests, one module per area. Every test opens its own store in a temp folder.

use std::fs;
use std::path::Path as FsPath;

use crate::*;

mod groups;
mod lifecycle;
mod misc;
mod setup;
mod tasks;

pub(super) fn join(dir: &FsPath, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

pub(super) fn s(p: &FsPath) -> String {
    p.to_string_lossy().into_owned()
}

pub(super) fn is_ulid(id: &str) -> bool {
    id.len() == 26 && id.chars().all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c))
}

pub(super) fn open_task_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let db = Store::open(&join(dir.path(), "kiki.yaml")).unwrap();
    (dir, db)
}

pub(super) fn exists(p: impl AsRef<FsPath>) -> bool {
    fs::symlink_metadata(p).is_ok()
}
