//! Nx projects ("services") inside a repository: which one a path belongs to and which ones an agent may use.

use std::fs;
use std::io::Read;
use std::sync::OnceLock;

use kiki_util::path as fpath;
use regex::Regex;
use serde_json::Value;

use super::model::{Decision, Policy};
use super::paths::{abs_path, inside};

/// Matches an Nx project name stored for a repository.
fn service_name() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$").expect("service name pattern"))
}

impl Policy {
    /// Blocks a path inside an Nx project the repository did not select.
    pub(super) fn veto_service_path(&self, path: &str) -> Option<Decision> {
        let (root, names) = self.service_limit(path)?;
        match service_at(path, &root) {
            Some(name) if !names.contains(&name) => Some(Decision::block(format!(
                "Guardrail: this agent cannot use the service {name}. This repository is set to {}.",
                service_list(&names)
            ))),
            _ => None,
        }
    }

    /// The deepest repository root with a service limit that holds `path`, with its allowed services.
    fn service_limit(&self, path: &str) -> Option<(String, Vec<String>)> {
        let mut root = String::new();
        let mut names: Vec<String> = Vec::new();
        for (candidate, allowed) in &self.services {
            if !inside(candidate, path) {
                continue;
            }
            if root.is_empty() || abs_path(candidate).len() > root.len() {
                root = abs_path(candidate);
                names = allowed.clone();
            }
        }
        if root.is_empty() {
            return None;
        }
        Some((root, names))
    }
}

/// The Nx project that holds `path`: the name in the nearest `project.json` between the path and `root`.
fn service_at(path: &str, root: &str) -> Option<String> {
    let path = abs_path(path);
    let root = abs_path(root);
    if path.is_empty() || root.is_empty() || !inside(&root, &path) {
        return None;
    }
    let mut dir = match fs::metadata(&path) {
        Ok(info) if info.is_dir() => path.clone(),
        _ => fpath::dir(&path),
    };
    loop {
        if !inside(&root, &dir) {
            return None;
        }
        if let Some(name) = read_service_name(&fpath::join(&[&dir, "project.json"])) {
            return Some(name);
        }
        if dir == root {
            return None;
        }
        let parent = fpath::dir(&dir);
        if parent == dir {
            return None;
        }
        dir = parent;
    }
}

/// The valid project name in a `project.json`, or `None` when the file is missing, unreadable, or has no valid name.
fn read_service_name(path: &str) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(256 * 1024).read_to_end(&mut buf).ok()?;
    // Only the first JSON value counts.
    let mut stream = serde_json::Deserializer::from_slice(&buf).into_iter::<Value>();
    let doc = stream.next()?.ok()?;
    let name = match doc {
        Value::Object(map) => {
            // Key names match ignoring case; the last match wins.
            let mut name = String::new();
            for (key, value) in &map {
                if !key.eq_ignore_ascii_case("name") {
                    continue;
                }
                match value {
                    Value::String(s) => name = s.clone(),
                    Value::Null => {}
                    _ => return None,
                }
            }
            name
        }
        Value::Null => String::new(),
        _ => return None,
    };
    let name = name.trim().to_string();
    if !service_name().is_match(&name) {
        return None;
    }
    Some(name)
}

fn service_list(names: &[String]) -> String {
    let mut sorted = names.to_vec();
    sorted.sort();
    sorted.join(", ")
}
