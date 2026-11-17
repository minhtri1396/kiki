//! Opening pages in the system browser, and the Jira page of a ticket.

use std::process::Command;
use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use regex::Regex;

static JIRA_KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9]{1,9}-[0-9]+$").unwrap());

/// The only addresses the console may open in the system browser.
const INDEX_PAGES: [&str; 3] = [
    "https://obsidian.md/help/cli",
    "https://github.com/colbymchenry/codegraph",
    "https://id.atlassian.com/manage-profile/security/api-tokens",
];

/// The page to open for `raw`, or an error when the console may not open it.
pub(crate) fn index_page(raw: &str) -> Result<&str> {
    let page = raw.trim();
    if !INDEX_PAGES.contains(&page) {
        return Err(anyhow!("that page cannot be opened"));
    }
    Ok(page)
}

/// Opens `page` with the operating system's default browser.
pub(crate) fn open_in_browser(page: &str) -> Result<()> {
    let mut cmd = page_command(page)?;
    let status = cmd.status().map_err(|e| anyhow!("{e}"))?;
    if !status.success() {
        return match status.code() {
            Some(code) => Err(anyhow!("exit status {code}")),
            None => Err(anyhow!("{status}")),
        };
    }
    Ok(())
}

/// The Jira issue page for `key` on the saved site.
pub(crate) fn jira_browse_page(site: &str, key: &str) -> Result<String> {
    let key = key.trim().to_uppercase();
    if !JIRA_KEY_PATTERN.is_match(&key) {
        return Err(anyhow!("That ticket is not on a group."));
    }
    let site_error = || anyhow!("Set the Jira site URL in Settings.");
    let mut parsed = url::Url::parse(site.trim()).map_err(|_| site_error())?;
    let has_user = !parsed.username().is_empty() || parsed.password().is_some();
    let has_query = parsed.query().map(|q| !q.is_empty()).unwrap_or(false);
    let has_fragment = parsed.fragment().map(|f| !f.is_empty()).unwrap_or(false);
    if parsed.host_str().unwrap_or("").is_empty()
        || (parsed.scheme() != "https" && parsed.scheme() != "http")
        || has_user
        || has_query
        || has_fragment
    {
        return Err(site_error());
    }
    let path = parsed.path();
    let path = path.strip_suffix('/').unwrap_or(path).to_string();
    parsed.set_path(&format!("{path}/browse/{key}"));
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

pub(crate) fn page_command(page: &str) -> Result<Command> {
    if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        cmd.arg(page);
        Ok(cmd)
    } else if cfg!(target_os = "windows") {
        let mut cmd = Command::new("rundll32");
        cmd.arg("url.dll,FileProtocolHandler").arg(page);
        Ok(cmd)
    } else {
        if kiki_util::look_path("xdg-open").is_none() {
            return Err(anyhow!("no browser opener was found"));
        }
        let mut cmd = Command::new("xdg-open");
        cmd.arg(page);
        Ok(cmd)
    }
}
