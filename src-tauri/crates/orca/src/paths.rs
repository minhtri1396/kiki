//! Path comparison helpers on top of `kiki_util::path`.

use anyhow::Result;
use kiki_util::path;

/// Reports whether two paths name the same place: lexically, or after resolving symlinks.
pub(crate) fn same_path(a: &str, b: &str) -> bool {
    let a = path::clean(a);
    let b = path::clean(b);
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(&a), std::fs::canonicalize(&b)) {
        (Ok(aa), Ok(bb)) => path::clean(&aa.to_string_lossy()) == path::clean(&bb.to_string_lossy()),
        _ => false,
    }
}

/// `p` made absolute against the current directory and cleaned.
pub(crate) fn abs_path(p: &str) -> Result<String> {
    if path::is_abs(p) {
        return Ok(path::clean(p));
    }
    let cwd = std::env::current_dir()?;
    Ok(path::join(&[&cwd.to_string_lossy(), p]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_is_lexical_first() {
        assert!(same_path("/a/b/../c", "/a/c"));
        assert!(!same_path("/definitely/not/here", "/also/not/here"));
    }

    #[test]
    fn abs_path_keeps_absolute_paths() {
        assert_eq!(abs_path("/a/./b").unwrap(), "/a/b");
        assert!(abs_path("rel").unwrap().ends_with("/rel"));
    }
}
