//! Finding the Git work tree of a folder and expressing app paths relative to it.

use crate::entries::Entry;
use anyhow::Result;
use kiki_util::path;
use std::io::ErrorKind;

/// A path the app writes, as a `.gitignore` pattern.
#[derive(Clone, Debug)]
pub(crate) struct Want {
    /// The line to add: `/rel` or `/rel/` for a directory.
    pub(crate) pattern: String,
    /// The path relative to the work tree root.
    pub(crate) rel: String,
    pub(crate) dir: bool,
}

/// Makes `p` absolute against the current directory and cleans it.
fn abs(p: &str) -> Result<String> {
    if path::is_abs(p) {
        return Ok(path::clean(p));
    }
    let cwd = std::env::current_dir()?;
    Ok(path::join(&[&cwd.to_string_lossy(), p]))
}

/// The relative path from `base` to `target`, both cleaned absolute paths.
fn rel(base: &str, target: &str) -> String {
    if base == target {
        return ".".to_string();
    }
    let b: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    let t: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
    let common = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<&str> = vec![".."; b.len() - common];
    parts.extend(&t[common..]);
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

/// Walks up from dir looking for `.git`. Returns the work tree root when found.
pub(crate) fn find_work_tree(dir: &str) -> Result<Option<String>> {
    let mut dir = abs(dir)?;
    loop {
        match std::fs::symlink_metadata(std::path::Path::new(&dir).join(".git")) {
            Ok(_) => return Ok(Some(dir)),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let parent = path::dir(&dir);
        if parent == dir {
            return Ok(None);
        }
        dir = parent;
    }
}

/// The pattern for `entry`, or `None` when it is blank or outside the work tree at `root`.
pub(crate) fn want_for(root: &str, entry: &Entry) -> Result<Option<Want>> {
    let given = entry.path.trim();
    if given.is_empty() {
        return Ok(None);
    }
    let rel = rel(root, &abs(given)?);
    if rel == "." || rel.is_empty() || rel == ".." || rel.starts_with("../") {
        return Ok(None);
    }
    let mut pattern = format!("/{rel}");
    if entry.dir {
        pattern.push('/');
    }
    Ok(Some(Want {
        pattern,
        rel,
        dir: entry.dir,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_walks_up_and_down() {
        assert_eq!(rel("/a/b", "/a/b"), ".");
        assert_eq!(rel("/a/b", "/a/b/c/d"), "c/d");
        assert_eq!(rel("/a/b", "/a/x"), "../x");
        assert_eq!(rel("/a/b", "/"), "../..");
    }

    #[test]
    fn want_for_skips_paths_outside_the_root() {
        let dir = Entry {
            path: "/r/docs".to_string(),
            dir: true,
        };
        assert_eq!(want_for("/r", &dir).unwrap().unwrap().pattern, "/docs/");
        assert!(want_for(
            "/r",
            &Entry {
                path: "/other/x".into(),
                dir: false
            }
        )
        .unwrap()
        .is_none());
        assert!(want_for(
            "/r",
            &Entry {
                path: " ".into(),
                dir: false
            }
        )
        .unwrap()
        .is_none());
        assert!(want_for(
            "/r",
            &Entry {
                path: "/r".into(),
                dir: true
            }
        )
        .unwrap()
        .is_none());
    }
}
