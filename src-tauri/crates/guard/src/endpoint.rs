//! The address and repository folders each running KIKI publishes under `~/.kiki/guardrails` so hooks can find it.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::bail;
use kiki_util::json::marshal_indent;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::policy::abs_path;
use crate::settings::write_atomic;

fn workspace_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-z][a-z0-9_-]{0,31}$").expect("workspace pattern"))
}

/// One running KIKI the hook can ask.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub workspace: String,
    /// An empty list is written as `null`, the form already on users' disks.
    #[serde(default, serialize_with = "paths_out", deserialize_with = "paths_in")]
    pub paths: Vec<String>,
}

fn paths_out<S: Serializer>(paths: &[String], s: S) -> std::result::Result<S::Ok, S::Error> {
    if paths.is_empty() {
        s.serialize_none()
    } else {
        s.collect_seq(paths)
    }
}

fn paths_in<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

/// The local address hooks call.
pub fn listen_url(addr: &str) -> String {
    let mut addr = addr.trim().to_string();
    if addr.starts_with(':') {
        addr = format!("127.0.0.1{addr}");
    }
    format!("http://{}", addr.trim_end_matches('/'))
}

/// Writes the address and repository folders for one workspace.
pub fn publish(home: impl AsRef<Path>, mut ep: Endpoint) -> anyhow::Result<()> {
    ep.workspace = ep.workspace.trim().to_string();
    if ep.workspace.is_empty() {
        ep.workspace = "kiki".to_string();
    }
    if !workspace_pattern().is_match(&ep.workspace) {
        bail!("workspace is not valid");
    }
    ep.url = ep.url.trim().trim_end_matches('/').to_string();
    if ep.url.is_empty() {
        bail!("guardrail address is not set");
    }
    ep.paths = clean_list(&ep.paths);
    let raw = marshal_indent(&serde_json::to_value(&ep)?);
    write_atomic(
        &home
            .as_ref()
            .join(".kiki")
            .join("guardrails")
            .join(format!("{}.json", ep.workspace)),
        raw.as_bytes(),
    )
}

/// Reads the workspaces this machine has published, sorted by file name. Unreadable or address-less files are skipped.
pub fn load(home: impl AsRef<Path>) -> anyhow::Result<Vec<Endpoint>> {
    let dir = home.as_ref().join(".kiki").join("guardrails");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut files: Vec<(String, bool)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        files.push((
            entry.file_name().to_string_lossy().into_owned(),
            entry.file_type()?.is_dir(),
        ));
    }
    files.sort();
    let mut out = Vec::new();
    for (name, is_dir) in files {
        if is_dir || !name.ends_with(".json") {
            continue;
        }
        let raw = fs::read(dir.join(&name))?;
        let Ok(ep) = serde_json::from_slice::<Endpoint>(&raw) else {
            continue;
        };
        if ep.url.trim().is_empty() {
            continue;
        }
        out.push(ep);
    }
    Ok(out)
}

fn clean_list(paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for path in paths {
        let path = abs_path(path);
        if path.is_empty() || out.contains(&path) {
            continue;
        }
        out.push(path);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{p, tmp};

    #[test]
    fn listen_url_fills_in_the_local_host() {
        assert_eq!(listen_url(":8080"), "http://127.0.0.1:8080");
        assert_eq!(listen_url(" 10.0.0.1:1/ "), "http://10.0.0.1:1");
    }

    #[test]
    fn publish_then_load_round_trips_and_cleans_paths() {
        let home_dir = tmp();
        let home = p(&home_dir);
        publish(
            &home,
            Endpoint {
                url: "http://127.0.0.1:9/".into(),
                workspace: " ".into(),
                paths: vec!["/a/b/../c".into(), "/a/c".into(), "relative".into()],
            },
        )
        .unwrap();
        let got = load(&home).unwrap();
        assert_eq!(
            got,
            vec![Endpoint {
                url: "http://127.0.0.1:9".into(),
                workspace: "kiki".into(),
                paths: vec!["/a/c".into()]
            }]
        );
    }

    #[test]
    fn publish_rejects_bad_workspace_and_missing_address() {
        let home_dir = tmp();
        let home = p(&home_dir);
        let bad = Endpoint {
            url: "http://x".into(),
            workspace: "Bad Name".into(),
            paths: vec![],
        };
        assert_eq!(publish(&home, bad).unwrap_err().to_string(), "workspace is not valid");
        let none = Endpoint {
            workspace: "kiki".into(),
            ..Default::default()
        };
        assert_eq!(
            publish(&home, none).unwrap_err().to_string(),
            "guardrail address is not set"
        );
    }

    #[test]
    fn empty_paths_are_written_as_null() {
        let home_dir = tmp();
        let home = p(&home_dir);
        publish(
            &home,
            Endpoint {
                url: "http://x".into(),
                workspace: "w".into(),
                paths: vec![],
            },
        )
        .unwrap();
        let raw = fs::read_to_string(Path::new(&home).join(".kiki/guardrails/w.json")).unwrap();
        assert_eq!(
            raw,
            "{\n  \"paths\": null,\n  \"url\": \"http://x\",\n  \"workspace\": \"w\"\n}\n"
        );
    }

    #[test]
    fn load_of_a_missing_folder_is_empty() {
        let dir = tmp();
        assert!(load(p(&dir)).unwrap().is_empty());
    }
}
