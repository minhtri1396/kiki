//! Name and path rules: what can be a member folder, and what can be a relative agent file path.

use anyhow::{anyhow, Result};
use kiki_util::path;

/// The relative path from `base` to `targ`. Fails when one is absolute and the other is not, or when
/// `base` climbs out of itself.
pub(crate) fn relative(base: &str, targ: &str) -> Result<String> {
    let base_c = path::clean(base);
    let targ_c = path::clean(targ);
    if targ_c == base_c {
        return Ok(".".to_string());
    }
    if path::is_abs(&base_c) != path::is_abs(&targ_c) {
        return Err(anyhow!("Rel: can't make {targ} relative to {base}"));
    }
    let comps = |p: &str| -> Vec<String> {
        p.split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .map(|c| c.to_string())
            .collect()
    };
    let b = comps(&base_c);
    let t = comps(&targ_c);
    let common = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    if b[common..].iter().any(|c| c == "..") {
        return Err(anyhow!("Rel: can't make {targ} relative to {base}"));
    }
    let mut parts: Vec<String> = vec!["..".to_string(); b.len() - common];
    parts.extend(t[common..].iter().cloned());
    if parts.is_empty() {
        return Ok(".".to_string());
    }
    Ok(parts.join("/"))
}

/// Reports whether `name` can be one folder name.
pub(crate) fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\r', '\n', '\0'])
}

/// Reports whether either folder is inside the other (or they are the same).
pub(crate) fn overlaps(a: &str, b: &str) -> bool {
    within(a, b) || within(b, a)
}

fn within(root: &str, target: &str) -> bool {
    match relative(root, target) {
        Ok(r) => r != ".." && !r.starts_with("../"),
        Err(_) => false,
    }
}

/// Reports whether `target` is strictly below `root`.
pub(crate) fn inside(root: &str, target: &str) -> bool {
    match relative(root, target) {
        Ok(r) => r != "." && !r.starts_with("../") && r != "..",
        Err(_) => false,
    }
}

/// A relative agent file path with `/` separators, cleaned; fails for empty, absolute or escaping paths.
pub(crate) fn safe_rel(rel_in: &str) -> Result<String> {
    let rel_s = rel_in.trim().replace('\\', "/");
    if rel_s.is_empty() || rel_s.starts_with('/') || rel_s.contains('\0') {
        return Err(anyhow!("agent file path {:?} is not valid", rel_s));
    }
    let cleaned = path::clean(&rel_s);
    if cleaned == "." || cleaned == ".." || cleaned.starts_with("../") {
        return Err(anyhow!("agent file path {:?} is not valid", rel_s));
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths() {
        assert_eq!(relative("/a/b", "/a/b").unwrap(), ".");
        assert_eq!(relative("/a/b", "/a/b/c").unwrap(), "c");
        assert_eq!(relative("/a/b", "/a/c").unwrap(), "../c");
        assert!(relative("/a", "b").is_err());
        assert!(overlaps("/a", "/a/b") && overlaps("/a/b", "/a") && !overlaps("/a", "/b"));
        assert!(inside("/a", "/a/b") && !inside("/a", "/a") && !inside("/a/b", "/a"));
    }

    #[test]
    fn safe_names_and_rel_paths() {
        assert!(safe_name("alice") && !safe_name("") && !safe_name("..") && !safe_name("a/b"));
        assert_eq!(safe_rel(" a\\b/../c.md ").unwrap(), "a/c.md");
        assert!(safe_rel("/abs").is_err() && safe_rel("../x").is_err() && safe_rel("").is_err());
        assert!(safe_rel("a\0b").is_err() && safe_rel(".").is_err());
    }
}
