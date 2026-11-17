//! Listing Jira boards and Confluence spaces and pages for the Settings pickers.

use anyhow::{anyhow, Result};
use kiki_util::Ctx;
use serde::{Deserialize, Serialize};

use super::login::Login;
use super::AtlassianApi;
use crate::atlassian::Atlassian;
use crate::urlenc::encode_query;
use crate::wire::{clip_bytes, decode, nd, FlexId};

/// Hit is one Jira board, Confluence space, or Confluence page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
}

impl Hit {
    fn new(id: &str, name: &str, key: &str) -> Hit {
        Hit {
            id: id.trim().to_string(),
            name: name.trim().to_string(),
            key: key.trim().to_string(),
        }
    }

    /// A hit needs both an id and a name to be pickable.
    fn is_complete(&self) -> bool {
        !self.id.is_empty() && !self.name.is_empty()
    }
}

/// The most hits one list returns, and the most pages of spaces followed.
const MAX_HITS: usize = 50;
const MAX_SPACE_PAGES: usize = 4;

#[derive(Deserialize, Default)]
struct BoardItem {
    #[serde(default)]
    id: FlexId,
    #[serde(default, deserialize_with = "nd")]
    name: String,
}

#[derive(Deserialize, Default)]
struct BoardList {
    #[serde(default, deserialize_with = "nd")]
    values: Vec<BoardItem>,
}

#[derive(Deserialize, Default)]
struct SpaceItem {
    #[serde(default)]
    id: FlexId,
    #[serde(default, deserialize_with = "nd")]
    key: String,
    #[serde(default, deserialize_with = "nd")]
    name: String,
}

#[derive(Deserialize, Default)]
struct NextLink {
    #[serde(default, deserialize_with = "nd")]
    next: String,
}

#[derive(Deserialize, Default)]
struct SpaceList {
    #[serde(default, deserialize_with = "nd")]
    results: Vec<SpaceItem>,
    #[serde(default, rename = "_links", deserialize_with = "nd")]
    links: NextLink,
}

#[derive(Deserialize, Default)]
struct PageItem {
    #[serde(default)]
    id: FlexId,
    #[serde(default, deserialize_with = "nd")]
    title: String,
}

#[derive(Deserialize, Default)]
struct PageList {
    #[serde(default, deserialize_with = "nd")]
    results: Vec<PageItem>,
}

#[derive(Deserialize, Default)]
struct SearchContent {
    #[serde(default)]
    id: FlexId,
    #[serde(default, deserialize_with = "nd")]
    title: String,
}

#[derive(Deserialize, Default)]
struct SearchItem {
    #[serde(default, deserialize_with = "nd")]
    title: String,
    #[serde(default, deserialize_with = "nd")]
    content: SearchContent,
}

#[derive(Deserialize, Default)]
struct SearchList {
    #[serde(default, deserialize_with = "nd")]
    results: Vec<SearchItem>,
}

impl AtlassianApi {
    /// Boards lists Jira boards the saved token can see. query narrows the name.
    pub fn boards(&self, ctx: &Ctx, cfg: &Atlassian, query: &str) -> Result<Vec<Hit>> {
        let login = Login::for_service(cfg, &cfg.jira, "Jira")?;
        let mut values = vec![("maxResults", MAX_HITS.to_string())];
        let text = clip_query(query);
        if !text.is_empty() {
            values.push(("name", text));
        }
        let url = format!("{}/rest/agile/1.0/board?{}", login.origin, encode_query(values));
        let body = self.read_ok(ctx, &url, &login, "Jira boards")?;
        let list: BoardList = decode(&body).map_err(|e| anyhow!("Jira boards: {e}"))?;
        Ok(list
            .values
            .iter()
            .map(|item| Hit::new(&item.id.raw, &item.name, ""))
            .filter(Hit::is_complete)
            .collect())
    }

    /// Spaces lists Confluence spaces. query keeps names and keys that contain it.
    pub fn spaces(&self, ctx: &Ctx, cfg: &Atlassian, query: &str) -> Result<Vec<Hit>> {
        let login = Login::for_service(cfg, &cfg.confluence, "Confluence")?;
        let mut next = format!("{}/wiki/api/v2/spaces?limit=50&status=current", login.origin);
        let want = clip_query(query);
        let mut out: Vec<Hit> = Vec::new();
        let mut page = 0;
        while page < MAX_SPACE_PAGES && !next.is_empty() {
            let body = self.read_ok(ctx, &next, &login, "Confluence spaces")?;
            let list: SpaceList = decode(&body).map_err(|e| anyhow!("Confluence spaces: {e}"))?;
            for item in &list.results {
                let hit = Hit::new(&item.id.raw, &item.name, &item.key);
                if !hit.is_complete() || !matches(&want, &[&hit.name, &hit.key]) {
                    continue;
                }
                out.push(hit);
                if out.len() == MAX_HITS {
                    return Ok(out);
                }
            }
            if want.is_empty() {
                return Ok(out);
            }
            next = join_next(&login.origin, &list.links.next);
            page += 1;
        }
        Ok(out)
    }

    /// Pages lists pages in one Confluence space. query searches titles.
    pub fn pages(&self, ctx: &Ctx, cfg: &Atlassian, space_id: &str, space_key: &str, query: &str) -> Result<Vec<Hit>> {
        let login = Login::for_service(cfg, &cfg.confluence, "Confluence")?;
        let space_id = space_id.trim();
        let space_key = space_key.trim();
        if space_id.is_empty() && space_key.is_empty() {
            return Err(anyhow!("choose a Confluence space first"));
        }
        let query = clip_query(query);
        if !query.is_empty() && !space_key.is_empty() {
            return self.search_pages(ctx, &login, space_key, &query);
        }
        if space_id.is_empty() {
            return Err(anyhow!("choose a Confluence space first"));
        }
        let values = vec![
            ("space-id", space_id.to_string()),
            ("limit", MAX_HITS.to_string()),
            ("status", "current".to_string()),
            ("sort", "title".to_string()),
        ];
        let url = format!("{}/wiki/api/v2/pages?{}", login.origin, encode_query(values));
        let body = self.read_ok(ctx, &url, &login, "Confluence pages")?;
        let list: PageList = decode(&body).map_err(|e| anyhow!("Confluence pages: {e}"))?;
        Ok(list
            .results
            .iter()
            .map(|item| Hit::new(&item.id.raw, &item.title, ""))
            .filter(|hit| hit.is_complete() && matches(&query, &[&hit.name]))
            .collect())
    }

    fn search_pages(&self, ctx: &Ctx, login: &Login, space_key: &str, query: &str) -> Result<Vec<Hit>> {
        let cql = format!(
            "type=page AND space=\"{}\" AND title~\"{}\"",
            escape_cql(space_key),
            escape_cql(query)
        );
        let url = format!(
            "{}/wiki/rest/api/content/search?{}",
            login.origin,
            encode_query(vec![("cql", cql), ("limit", "25".to_string())])
        );
        let body = self.read_ok(ctx, &url, login, "Confluence pages")?;
        let list: SearchList = decode(&body).map_err(|e| anyhow!("Confluence pages: {e}"))?;
        Ok(list
            .results
            .iter()
            .map(|item| {
                let content_title = item.content.title.trim();
                let name = if content_title.is_empty() {
                    &item.title
                } else {
                    &item.content.title
                };
                Hit::new(&item.content.id.raw, name, "")
            })
            .filter(Hit::is_complete)
            .collect())
    }
}

/// True when the query is empty or any field contains it, ignoring case.
fn matches(query: &str, fields: &[&str]) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    fields.iter().any(|field| field.to_lowercase().contains(&query))
}

fn clip_query(query: &str) -> String {
    clip_bytes(query.trim(), 80)
}

fn escape_cql(text: &str) -> String {
    text.replace(['\\', '"'], "")
}

/// Turns the `next` link Confluence returns into an absolute URL ("" when there is no next page).
fn join_next(origin: &str, next: &str) -> String {
    let mut next = next.trim().to_string();
    if next.is_empty() {
        return String::new();
    }
    if next.starts_with("http://") || next.starts_with("https://") {
        return next;
    }
    if !next.starts_with('/') {
        next = format!("/{next}");
    }
    format!("{origin}{next}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_next_and_matches() {
        assert_eq!(join_next("https://x", "a/b"), "https://x/a/b");
        assert_eq!(join_next("https://x", "https://y/z"), "https://y/z");
        assert_eq!(join_next("https://x", " "), "");
        assert!(matches("", &["x"]) && matches("EN", &["Engineering"]) && !matches("zz", &["Engineering"]));
    }
}
