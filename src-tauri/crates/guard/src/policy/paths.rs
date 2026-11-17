//! Lexical and real-path checks for "is this path inside that root".

use std::fs;

use kiki_util::path as fpath;

/// Whether path is the root or a file under it. Symlinks are followed for the part that exists.
pub fn inside(root: &str, path: &str) -> bool {
    let root = abs_path(root);
    let path = abs_path(path);
    if root.is_empty() || path.is_empty() {
        return false;
    }
    if lexical_inside(&resolve_existing(&root), &resolve_existing(&path)) {
        return true;
    }
    lexical_inside(&root, &path)
}

pub(super) fn inside_any(roots: &[String], path: &str) -> bool {
    roots.iter().any(|root| inside(root, path))
}

fn lexical_inside(root: &str, path: &str) -> bool {
    let root = fpath::clean(root);
    let path = fpath::clean(path);
    if root == "." || path == "." || root.is_empty() || path.is_empty() {
        return false;
    }
    if path == root {
        return true;
    }
    // Both paths are clean and absolute, so the relative path climbs out of root
    // exactly when path is not under it.
    root == "/" || path.starts_with(&format!("{root}/"))
}

/// A path as a clean absolute path: `~` and `~/..` use `home`, relative paths use `cwd`.
/// Empty when it cannot be made absolute.
pub(super) fn expand_path(path: &str, cwd: &str, home: &str) -> String {
    let mut path = path.trim().to_string();
    if path.is_empty() {
        return String::new();
    }
    if path == "~" && !home.is_empty() {
        return abs_path(home);
    }
    if path.starts_with("~/") && !home.is_empty() {
        path = fpath::join(&[home, &path[2..]]);
    }
    if !fpath::is_abs(&path) {
        if cwd.is_empty() {
            return String::new();
        }
        path = fpath::join(&[cwd, &path]);
    }
    abs_path(&path)
}

/// A clean absolute path, or empty for an empty, `.`, or relative path.
pub(crate) fn abs_path(path: &str) -> String {
    let path = fpath::clean(path.trim());
    if path.is_empty() || path == "." || !fpath::is_abs(&path) {
        return String::new();
    }
    path
}

/// The path with symlinks resolved for its longest existing prefix; the missing rest is appended unchanged.
fn resolve_existing(path: &str) -> String {
    let path = fpath::clean(path);
    let mut suffix: Vec<String> = Vec::new();
    let mut cur = path.clone();
    loop {
        if fs::metadata(&cur).is_ok() {
            if let Ok(real) = fs::canonicalize(&cur) {
                cur = real.to_string_lossy().into_owned();
            }
            let mut parts: Vec<&str> = vec![cur.as_str()];
            parts.extend(suffix.iter().map(|s| s.as_str()));
            return fpath::clean(&fpath::join(&parts));
        }
        let parent = fpath::dir(&cur);
        if parent == cur {
            return path;
        }
        suffix.insert(0, fpath::base(&cur));
        cur = parent;
    }
}
