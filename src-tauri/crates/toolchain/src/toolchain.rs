//! The app repositories and the CodeGraph commands the console runs.

use kiki_util::path;

/// Kinds are the repositories the console stores.
pub const KINDS: &[&str] = &["backend", "website", "mobile", "document"];

/// Reports whether `kind` is one of the built-in app repositories.
pub fn known(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Reports whether `kind` can be stored as a repository id (`^[a-z][a-z0-9_]*$`).
pub fn repo_id(kind: &str) -> bool {
    let mut chars = kind.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `codegraph init <folder>`.
pub fn codegraph_init(folder: &str) -> Vec<String> {
    vec!["codegraph".into(), "init".into(), folder.into()]
}

/// `codegraph status <folder>`.
pub fn codegraph_status(folder: &str) -> Vec<String> {
    vec!["codegraph".into(), "status".into(), folder.into()]
}

/// The absolute path of a repository under the app root.
/// Document is the docs directory. The others use their kind as the directory name.
pub fn folder(root: &str, kind: &str) -> String {
    let name = if kind == "document" { "docs" } else { kind };
    path::join(&[root, name])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!(known("backend") && !known("docs"));
        assert!(repo_id("a_b1") && !repo_id("1a") && !repo_id("") && !repo_id("aB"));
        assert_eq!(folder("/r", "document"), "/r/docs");
        assert_eq!(folder("/r/", "backend"), "/r/backend");
        assert_eq!(codegraph_init("/x"), vec!["codegraph", "init", "/x"]);
    }
}
