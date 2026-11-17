//! Reading existing ignore rules: which wanted patterns are already covered.

use crate::worktree::Want;
use std::collections::HashSet;

pub(crate) const HEADER_COMMENT: &str = "# KIKI generated files";
pub(crate) const INDEX_HEADER: &str = "# KIKI index";

/// One active line of a `.gitignore`.
struct Rule {
    negated: bool,
    pattern: String,
}

/// The patterns in `wants` that the existing `text` does not already ignore, without repeats.
pub(crate) fn missing_patterns(text: &str, wants: &[Want]) -> Vec<String> {
    let mut active = ignore_lines(text);
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for item in wants {
        if item.pattern.is_empty() || !seen.insert(&item.pattern) {
            continue;
        }
        if ignored(&active, &item.rel, item.dir) {
            continue;
        }
        out.push(item.pattern.clone());
        active.push(item.pattern.clone());
    }
    out
}

fn ignore_lines(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n').map(|s| s.to_string()).collect()
}

/// Applies the lines in order; the last one that covers `rel` decides.
fn ignored(lines: &[String], rel: &str, dir: bool) -> bool {
    let mut on = false;
    for rule in lines.iter().filter_map(|line| parse_rule(line)) {
        if pattern_covers(&rule.pattern, rel, dir) {
            on = !rule.negated;
        }
    }
    on
}

/// `None` for blank lines, comments and a bare `!`.
fn parse_rule(line: &str) -> Option<Rule> {
    let mut line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    if line.starts_with("\\#") || line.starts_with("\\!") {
        line = &line[1..];
    }
    if let Some(rest) = line.strip_prefix('!') {
        let pattern = rest.trim();
        if pattern.is_empty() {
            return None;
        }
        return Some(Rule {
            negated: true,
            pattern: pattern.to_string(),
        });
    }
    Some(Rule {
        negated: false,
        pattern: line.to_string(),
    })
}

/// Reports whether a Git ignore pattern ignores rel.
/// rel uses forward slashes and has no trailing slash. Globs are not treated as a match.
fn pattern_covers(pattern: &str, rel: &str, dir: bool) -> bool {
    let pattern = pattern.trim();
    let dir_only = pattern.ends_with('/');
    let pattern = pattern.strip_suffix('/').unwrap_or(pattern);
    if pattern.is_empty() || pattern.contains(['*', '?', '[']) {
        return false;
    }
    let anchored = pattern.starts_with('/');
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        return false;
    }
    if anchored || pattern.contains('/') {
        if pattern == rel {
            return !dir_only || dir;
        }
        return rel.starts_with(&format!("{pattern}/"));
    }
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.iter().position(|part| *part == pattern) {
        Some(i) if i < parts.len() - 1 => true,
        Some(_) => !dir_only || dir,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_cover_paths_like_git() {
        assert!(pattern_covers("/docs/", "docs", true));
        assert!(!pattern_covers("/docs/", "docs", false));
        assert!(pattern_covers("docs", "a/docs/b", false));
        assert!(pattern_covers("/a/b", "a/b/c", false));
        assert!(!pattern_covers("*.log", "x.log", false));
        assert!(!pattern_covers("", "x", false));
    }

    #[test]
    fn negation_wins_when_it_comes_last() {
        let lines = vec!["kiki.yaml".to_string(), "!kiki.yaml".to_string(), "# c".to_string()];
        assert!(!ignored(&lines, "kiki.yaml", false));
        assert!(ignored(&lines[..1], "kiki.yaml", false));
        assert!(parse_rule("!").is_none() && parse_rule("# x").is_none());
        assert_eq!(parse_rule("\\#x").unwrap().pattern, "#x");
    }
}
