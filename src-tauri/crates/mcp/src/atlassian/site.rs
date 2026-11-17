//! Site URL helpers for Jira and Confluence.

/// Trims spaces and trailing slashes.
pub(crate) fn clean_url(raw: &str) -> String {
    let mut raw = raw.trim();
    while let Some(rest) = raw.strip_suffix('/') {
        raw = rest;
    }
    raw.to_string()
}

/// The pieces of a site URL that matter here: the lowercased scheme and the host
/// (with port, without user info).
pub(crate) struct Site {
    pub scheme: String,
    pub host: String,
    pub has_user: bool,
}

/// Splits `scheme://authority/...`; `None` for an unparsable URL or an empty host.
pub(crate) fn parse_site(raw: &str) -> Option<Site> {
    let parsed = url::Url::parse(raw).ok()?;
    let marker = raw.find("://")?;
    let rest = &raw[marker + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let (has_user, host) = match authority.rfind('@') {
        Some(at) => (true, &authority[at + 1..]),
        None => (false, authority),
    };
    if host.is_empty() {
        return None;
    }
    Some(Site {
        scheme: parsed.scheme().to_string(),
        host: host.to_string(),
        has_user,
    })
}

pub(super) fn validate_site(raw: &str) -> std::result::Result<(), String> {
    match parse_site(raw) {
        Some(site) if (site.scheme == "https" || site.scheme == "http") && !site.has_user => Ok(()),
        _ => Err("must be an http or https URL".to_string()),
    }
}
