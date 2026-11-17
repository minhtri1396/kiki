//! Which addresses `git clone` may be given.

/// An address KIKI accepts: `https`, `http`, `ssh` or `git` with a host, `git@host:path`, or `file:///absolute`.
/// It never starts with `-` (that would be read as an option) and has no whitespace.
pub(crate) fn valid_git_url(raw: &str) -> bool {
    if raw.is_empty() || raw.contains([' ', '\t', '\r', '\n']) || raw.starts_with('-') {
        return false;
    }
    if let Some(rest) = raw.strip_prefix("git@") {
        return match rest.split_once(':') {
            Some((host, path)) => !host.is_empty() && !host.contains('/') && !path.is_empty(),
            None => false,
        };
    }
    let Ok(parsed) = url::Url::parse(raw) else { return false };
    let host = parsed.host_str().unwrap_or("");
    if host.is_empty() && parsed.scheme() != "file" {
        return false;
    }
    match parsed.scheme() {
        "https" | "http" | "ssh" | "git" => !host.is_empty(),
        "file" => {
            // `file:foo` has no path of its own; only `file:` followed by an absolute path counts.
            let rest = &raw[raw.find(':').map(|i| i + 1).unwrap_or(0)..];
            rest.starts_with('/') && !parsed.path().is_empty()
        }
        _ => false,
    }
}
