//! Small helpers shared by the server modules.

/// Reports whether `path` is a directory.
pub(crate) fn is_dir(path: &str) -> bool {
    std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}
