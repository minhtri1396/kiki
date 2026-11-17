//! Lexical path helpers (no filesystem access): normalise, join, split. Unix separators only.

/// Removes `.`, empty parts and `..` segments lexically; `""` becomes `"."`.
pub fn clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let rooted = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if let Some(last) = stack.last() {
                    if *last != ".." {
                        stack.pop();
                        continue;
                    }
                }
                if !rooted {
                    stack.push("..");
                }
            }
            p => stack.push(p),
        }
    }
    let body = stack.join("/");
    if rooted {
        format!("/{body}")
    } else if body.is_empty() {
        ".".to_string()
    } else {
        body
    }
}

/// Joins the non-empty parts with `/` and cleans the result; no parts give `""`.
pub fn join(parts: &[&str]) -> String {
    let joined: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if joined.is_empty() {
        return String::new();
    }
    clean(&joined.join("/"))
}

/// Everything before the last `/`, cleaned (`"."` when there is none).
pub fn dir(path: &str) -> String {
    let end = path.rfind('/').map(|i| i + 1).unwrap_or(0);
    clean(&path[..end])
}

/// The last path element (`"."` for an empty path, `"/"` for the root).
pub fn base(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        Some(i) => trimmed[i + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// True for paths that start with `/`.
pub fn is_abs(path: &str) -> bool {
    path.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_semantics() {
        assert_eq!(clean(""), ".");
        assert_eq!(clean("/a/b/../c/./d/"), "/a/c/d");
        assert_eq!(clean("/../a"), "/a");
        assert_eq!(clean("a/../.."), "..");
        assert_eq!(join(&["/a", "b/../c"]), "/a/c");
        assert_eq!(dir("/a"), "/");
        assert_eq!(dir("/a/b"), "/a");
        assert_eq!(dir(""), ".");
        assert_eq!(base("/a/b"), "b");
        assert_eq!(base("/"), "/");
    }
}
