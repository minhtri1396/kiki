//! Per-repository files in `.kiki/repos`, and the Store methods that edit them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;

use anyhow::{anyhow, Result};
use kiki_console as console;
use kiki_toolchain as toolchain;

use kiki_util::path::{clean, dir, is_abs, join};
use kiki_yaml::{Mapping, Value};

use super::lenient::{get_bool, get_str, get_strings, read_map};
use super::{read_doc, write_doc, RepoDoc};
use crate::fsutil;
use crate::store::Store;

/// One `repos/<id>.yaml` file.
#[derive(Debug, Clone, Default)]
pub(crate) struct RepoFile {
    pub id: String,
    pub name: String,
    pub folder: String,
    /// `None` when the key was missing or null.
    pub services: Option<Vec<String>>,
    /// `None` when the key was missing or null.
    pub divided: Option<bool>,
    pub index: String,
    pub sparse: String,
}

/// Decodes one repository file: `id` falls back to the older `kind` key.
fn parse_repo_file(raw: &str) -> Result<RepoFile> {
    let map = read_map(raw)?;
    let mut id = get_str(&map, "id").trim().to_string();
    if id.is_empty() {
        id = get_str(&map, "kind").trim().to_string();
    }
    Ok(RepoFile {
        id,
        name: get_str(&map, "name").trim().to_string(),
        folder: get_str(&map, "folder"),
        services: get_strings(&map, "services")?,
        divided: get_bool(&map, "divided")?,
        index: get_str(&map, "index"),
        sparse: get_str(&map, "sparse"),
    })
}

impl Store {
    /// SaveRepoIndex records obsidian or codegraph on one repository file.
    /// An empty index removes it. The other repositories in snap keep the index already on them.
    pub fn save_repo_index(&self, snap: &console::Snapshot, kind: &str, index: &str) -> Result<()> {
        let _guard = self.lock();
        if !toolchain::repo_id(kind) {
            return Err(anyhow!("unknown repository"));
        }
        if !index.is_empty() && index != "obsidian" && index != "codegraph" {
            return Err(anyhow!("index must be obsidian or codegraph"));
        }
        let mut snap = snap.clone();
        let mut found = false;
        for repo in snap.repos.iter_mut() {
            if repo.kind != kind {
                continue;
            }
            repo.index = index.to_string();
            found = true;
        }
        if !found {
            return Err(anyhow!("unknown repository"));
        }
        write_repo_files(&self.path, &repos_for_files(&snap))
    }

    /// SaveRepoFolder points one repository at an existing directory.
    /// The other repositories keep the folder already saved for them.
    pub fn save_repo_folder(&self, snap: &console::Snapshot, kind: &str, folder: &str) -> Result<()> {
        let _guard = self.lock();
        if !toolchain::repo_id(kind) {
            return Err(anyhow!("unknown repository"));
        }
        let folder = clean(folder.trim());
        if !is_abs(&folder) {
            return Err(anyhow!("folder must be an absolute path"));
        }
        match fs::metadata(&folder) {
            Ok(info) if info.is_dir() => {}
            _ => return Err(anyhow!("folder {folder} does not exist")),
        }
        let mut snap = snap.clone();
        let mut found = false;
        for repo in snap.repos.iter_mut() {
            if repo.kind != kind {
                continue;
            }
            repo.folder = folder.clone();
            found = true;
        }
        if !found {
            return Err(anyhow!("unknown repository"));
        }
        write_repo_files(&self.path, &repos_for_files(&snap))?;
        let mut doc = read_doc(&self.path)?;
        let mut updated = false;
        for repo in doc.repos.iter_mut() {
            if repo.kind != kind {
                continue;
            }
            repo.folder = folder.clone();
            updated = true;
        }
        if !updated {
            doc.repos.push(RepoDoc {
                kind: kind.to_string(),
                folder,
                services: Vec::new(),
            });
        }
        write_doc(&self.path, &doc)
    }

    /// SaveRepoSparse records the sparse-checkout folder on one repository file.
    /// An empty path clears it. The other repositories keep the path already on them.
    pub fn save_repo_sparse(&self, snap: &console::Snapshot, kind: &str, sparse: &str) -> Result<()> {
        let _guard = self.lock();
        if !toolchain::repo_id(kind) {
            return Err(anyhow!("unknown repository"));
        }
        let cleaned = console::sparse_path(sparse)?;
        let mut snap = snap.clone();
        let mut found = false;
        for repo in snap.repos.iter_mut() {
            if repo.kind != kind {
                continue;
            }
            repo.sparse = cleaned.clone();
            found = true;
        }
        if !found {
            return Err(anyhow!("unknown repository"));
        }
        write_repo_files(&self.path, &repos_for_files(&snap))
    }
}

/// RemoveIndexFolder deletes the index directory inside a repository.
/// codegraph removes .codegraph. obsidian removes .obsidian. A missing directory is left as success.
pub fn remove_index_folder(repo_folder: &str, index: &str) -> Result<()> {
    let name = index_dir_name(index)?;
    let repo_folder = repo_folder.trim();
    if repo_folder.is_empty() || !is_abs(repo_folder) {
        return Err(anyhow!("folder is not set"));
    }
    let repo_folder = clean(repo_folder);
    let target = join(&[&repo_folder, name]);
    let info = match fs::symlink_metadata(&target) {
        Ok(info) => info,
        Err(err) => {
            if fsutil::not_exist(&err) {
                return Ok(());
            }
            return Err(err.into());
        }
    };
    if info.file_type().is_symlink() {
        return Err(anyhow!("{name} is a link"));
    }
    if !info.is_dir() {
        return Err(anyhow!("{name} is not a folder"));
    }
    fs::remove_dir_all(&target)?;
    Ok(())
}

fn index_dir_name(index: &str) -> Result<&'static str> {
    match index {
        "codegraph" => Ok(".codegraph"),
        "obsidian" => Ok(".obsidian"),
        _ => Err(anyhow!("this repository has no index")),
    }
}

fn repos_dir(setup_path: &str) -> String {
    join(&[&dir(setup_path), "repos"])
}

fn clean_index(repo: &mut console::Repo) {
    if repo.index != "obsidian" && repo.index != "codegraph" {
        repo.index = String::new();
    }
}

pub(crate) fn repos_for_files(snap: &console::Snapshot) -> Vec<console::Repo> {
    let mut by_kind: HashMap<&str, &console::Repo> = HashMap::new();
    for repo in &snap.repos {
        by_kind.insert(repo.kind.as_str(), repo);
    }
    let mut out: Vec<console::Repo> = Vec::new();
    for kind in toolchain::KINDS {
        let Some(saved) = by_kind.get(kind) else { continue };
        let mut repo = (*saved).clone();
        repo.kind = kind.to_string();
        if repo.kind == "document" && !repo.divided {
            repo.services = Vec::new();
        }
        clean_index(&mut repo);
        out.push(repo);
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut extras: Vec<console::Repo> = Vec::new();
    for repo in &snap.repos {
        if toolchain::known(&repo.kind) || !toolchain::repo_id(&repo.kind) || seen.contains(&repo.kind) {
            continue;
        }
        seen.insert(repo.kind.clone());
        let mut repo = repo.clone();
        clean_index(&mut repo);
        extras.push(repo);
    }
    extras.sort_by(|a, b| a.kind.cmp(&b.kind));
    out.extend(extras);
    out
}

pub(crate) fn merge_repo_files(setup_path: &str, mut snap: console::Snapshot) -> Result<console::Snapshot> {
    let files = read_repo_files(setup_path)?;
    for repo in snap.repos.iter_mut() {
        let Some(file) = files.get(&repo.kind) else { continue };
        if !file.name.is_empty() {
            repo.name = file.name.clone();
        }
        repo.index = file.index.clone();
        let folder = file.folder.trim();
        if !folder.is_empty() && is_abs(folder) {
            repo.folder = clean(folder);
        }
        if let Ok(sparse) = console::sparse_path(&file.sparse) {
            repo.sparse = sparse;
        }
        repo.divided = file.divided.unwrap_or(false);
        if repo.kind != "document" || repo.divided {
            if let Some(services) = &file.services {
                repo.services = services.clone();
            }
        }
    }
    Ok(snap)
}

fn read_repo_files(setup_path: &str) -> Result<BTreeMap<String, RepoFile>> {
    let dir = repos_dir(setup_path);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) => {
            if fsutil::not_exist(&err) {
                return Ok(BTreeMap::new());
            }
            return Err(err.into());
        }
    };
    // Entries are read in file name order.
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".yaml") {
            continue;
        }
        names.push(name);
    }
    names.sort();
    let mut out = BTreeMap::new();
    for name in names {
        let raw = fs::read(join(&[&dir, &name]))?;
        let text = String::from_utf8_lossy(&raw).into_owned();
        let mut doc = parse_repo_file(&text).map_err(|err| anyhow!("read {name}: {err}"))?;
        if !toolchain::repo_id(&doc.id) {
            doc.id = name.strip_suffix(".yaml").unwrap_or(&name).to_string();
        }
        if !toolchain::repo_id(&doc.id) {
            continue;
        }
        if doc.index != "obsidian" && doc.index != "codegraph" {
            doc.index = String::new();
        }
        out.insert(doc.id.clone(), doc);
    }
    Ok(out)
}

pub(crate) fn write_repo_files(setup_path: &str, repos: &[console::Repo]) -> Result<()> {
    let dir = repos_dir(setup_path);
    fsutil::mkdir_all(&dir, 0o755)?;
    for repo in repos {
        if !toolchain::repo_id(&repo.kind) {
            continue;
        }
        let text = repo_file_text(repo);
        let file = join(&[&dir, &format!("{}.yaml", repo.kind)]);
        match fs::read(&file) {
            Ok(existing) => {
                if existing == text.as_bytes() {
                    continue;
                }
            }
            Err(err) => {
                if !fsutil::not_exist(&err) {
                    return Err(err.into());
                }
            }
        }
        let tmp = format!("{file}.tmp");
        fsutil::write_file(&tmp, text.as_bytes(), 0o644)?;
        fs::rename(&tmp, &file)?;
    }
    Ok(())
}

fn repo_file_text(repo: &console::Repo) -> String {
    let services: Vec<String> = if repo.kind == "document" && !repo.divided {
        Vec::new()
    } else {
        repo.services.clone()
    };
    let index = if repo.index == "obsidian" || repo.index == "codegraph" {
        repo.index.as_str()
    } else {
        ""
    };
    let sparse = console::sparse_path(&repo.sparse).unwrap_or_default();
    let entries = Mapping::new()
        .set("id", repo.kind.as_str())
        .set("name", repo.title())
        .set_unless_empty("folder", repo.folder.trim())
        .set_if(repo.divided, "divided", true)
        .set("services", Value::Seq(services.into_iter().map(Value::from).collect()))
        .set_unless_empty("index", index)
        .set_unless_empty("sparse", &sparse);
    let header = format!(
        "# Repository {}. KIKI stores this in .kiki/repos. The index is what the launch prompt tells the model to query.\n",
        repo.kind
    );
    format!("{header}{}", entries.emit())
}
