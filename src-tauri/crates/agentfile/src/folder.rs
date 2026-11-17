//! Operations on an agent folder: resolve its path, list, find and save agent files, and rewrite repository ids.

use std::collections::{BTreeMap, HashMap};
use std::fs;

use anyhow::{anyhow, Result};
use kiki_agentdef as agentdef;
use kiki_util::path::{clean, dir as dir_of, is_abs, join};

use crate::disk::{category_files, not_exist, path_err, remove_empty, write_atomic};
use crate::model::{ErrExists, ErrNotFound, File};
use crate::normalize::normalize_file;
use crate::parse::{parse, Header};
use crate::serialize;

/// Returns the agent folder. An empty path uses .kiki/agents next to the app.
pub fn resolve(root: &str, raw: &str) -> Result<String> {
    let root = root.trim();
    if root.is_empty() || !is_abs(root) {
        return Err(anyhow!("app root is not set"));
    }
    let mut raw = raw.trim().to_string();
    if raw.is_empty() {
        raw = join(&[root, ".kiki", "agents"]);
    }
    if raw.contains(['\r', '\n']) {
        return Err(anyhow!("agent path is invalid"));
    }
    if !is_abs(&raw) {
        return Err(anyhow!("agent path must be an absolute path"));
    }
    let clean = clean(&raw);
    match fs::metadata(&clean) {
        Ok(info) => {
            if !info.is_dir() {
                return Err(anyhow!("agent path is not a folder"));
            }
        }
        Err(e) => {
            if !not_exist(&e) {
                return Err(path_err("stat", &clean, &e));
            }
        }
    }
    Ok(clean)
}

/// Reads the agent files in category folders. Catalog agents come first in catalog order, then the rest by
/// category and id.
pub fn list(dir: &str) -> Result<Vec<File>> {
    let dir = clean(dir.trim());
    let paths = category_files(&dir)?;
    let mut files: Vec<File> = Vec::with_capacity(paths.len());
    for (path, category) in &paths {
        if let Some(file) = read_file(path, category)? {
            files.push(file);
        }
    }
    let order: HashMap<&str, usize> = agentdef::CATALOG
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id.as_str(), i))
        .collect();
    // The keys below are total, so the order does not depend on the sort being stable.
    files.sort_by(|a, b| {
        let left = order.get(a.id.as_str());
        let right = order.get(b.id.as_str());
        match (left, right) {
            (Some(l), Some(r)) => return l.cmp(r),
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            (None, None) => {}
        }
        if a.category != b.category {
            return a.category.cmp(&b.category);
        }
        a.id.cmp(&b.id)
    });
    Ok(files)
}

/// Returns the agent file with this id, or `Ok(None)` when there is none.
pub fn find(dir: &str, id: &str) -> Result<Option<File>> {
    let files = list(dir)?;
    Ok(files.into_iter().find(|file| file.id == id))
}

/// Writes one agent file. `current_id` is the id already stored on disk.
/// A new id or category moves the file. The prompt body is stored as given.
pub fn save(dir: &str, current_id: &str, next: &File) -> Result<()> {
    let dir = clean(dir.trim());
    if dir.is_empty() || !is_abs(&dir) {
        return Err(anyhow!("agent path is not set"));
    }
    let next = normalize_file(next)?;
    let current = locate_current(&dir, current_id, &next.id)?;
    let dest = join(&[&dir, &next.category, &format!("{}.md", next.id)]);
    match fs::metadata(&dest) {
        Ok(info) => {
            if !info.is_dir() && clean(&dest) != clean(&current.path) {
                return Err(ErrExists.into());
            }
        }
        Err(e) => {
            if !not_exist(&e) {
                return Err(path_err("stat", &dest, &e));
            }
        }
    }
    write_atomic(&dest, &serialize::format(&header_of(&next), &next.prompt))?;
    if clean(&dest) != clean(&current.path) {
        if let Err(e) = fs::remove_file(&current.path) {
            if !not_exist(&e) {
                return Err(path_err("remove", &current.path, &e));
            }
        }
        remove_empty(&dir_of(&current.path), &dir);
    }
    Ok(())
}

/// The stored file for `current_id`; another file already using `next_id` is [`ErrExists`], and no file for
/// `current_id` is [`ErrNotFound`].
fn locate_current(dir: &str, current_id: &str, next_id: &str) -> Result<File> {
    let mut current = None;
    for file in list(dir)? {
        if file.id == current_id {
            current = Some(file);
            continue;
        }
        if file.id == next_id {
            return Err(ErrExists.into());
        }
    }
    current.ok_or_else(|| ErrNotFound.into())
}

fn header_of(file: &File) -> Header {
    Header {
        id: file.id.clone(),
        name: file.name.clone(),
        description: file.description.clone(),
        enabled: Some(file.enabled),
        ai_tool: file.ai_tool.clone(),
        model: file.model.clone(),
        repos: file.repos.clone(),
        works_with: file.works_with.clone(),
        mcps: file.mcps.clone(),
        tools: file.tools.clone(),
        ..Default::default()
    }
}

/// Reads one agent file. `Ok(None)` means the file is not an agent file (no readable front matter).
fn read_file(path: &str, category: &str) -> Result<Option<File>> {
    let raw = fs::read(path).map_err(|e| path_err("open", path, &e))?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (meta, body) = match parse(&text) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    let mut id = meta.id.trim().to_string();
    if id.is_empty() {
        let base = path.rsplit('/').next().unwrap_or(path);
        id = match base.rfind('.') {
            Some(i) => base[..i].to_string(),
            None => base.to_string(),
        };
    }
    Ok(Some(File {
        id,
        name: meta.name.trim().to_string(),
        description: meta.description.trim().to_string(),
        category: category.to_string(),
        enabled: meta.enabled.unwrap_or(true),
        ai_tool: meta.ai_tool.trim().to_string(),
        model: meta.model.trim().to_string(),
        repos: meta.repos,
        works_with: meta.works_with,
        mcps: meta.mcps,
        tools: meta.tools,
        prompt: body,
        path: path.to_string(),
    }))
}

/// A repository id replacement table for [`replace_repos`]: `HashMap<String, String>`
/// or `BTreeMap<String, String>`.
pub trait Replacements {
    fn is_empty(&self) -> bool;
    fn replacement(&self, kind: &str) -> Option<&str>;
}

impl Replacements for HashMap<String, String> {
    fn is_empty(&self) -> bool {
        HashMap::is_empty(self)
    }
    fn replacement(&self, kind: &str) -> Option<&str> {
        self.get(kind).map(|s| s.as_str())
    }
}

impl Replacements for BTreeMap<String, String> {
    fn is_empty(&self) -> bool {
        BTreeMap::is_empty(self)
    }
    fn replacement(&self, kind: &str) -> Option<&str> {
        self.get(kind).map(|s| s.as_str())
    }
}

/// Rewrites repository ids in the agent files under `dir`.
/// Agents that do not use a replaced id are left unchanged.
pub fn replace_repos<R: Replacements>(dir: &str, replacements: &R) -> Result<()> {
    if replacements.is_empty() {
        return Ok(());
    }
    let files = list(dir)?;
    for mut file in files {
        let mut changed = false;
        for repo in file.repos.iter_mut() {
            let next = replacements.replacement(&repo.kind).unwrap_or("").trim().to_string();
            if next.is_empty() || next == repo.kind {
                continue;
            }
            repo.kind = next;
            changed = true;
        }
        if !changed {
            continue;
        }
        save(dir, &file.id.clone(), &file)?;
    }
    Ok(())
}
