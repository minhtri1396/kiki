//! Copies chosen agent files from one agents folder onto another.
//! Agents that were not chosen stay where they are. A chosen agent that already lives at a different path is
//! removed from that old path after the new file is in place.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{anyhow, Result};
use kiki_agentfile as agentfile;
use kiki_util::path;

use super::files::mkdir_all;

/// The agents in `files` whose ids are in `ids`.
/// A blank id is ignored. No remaining id, or an id that is not in `files`, is an error.
pub(crate) fn keep(files: Vec<agentfile::File>, ids: &[String]) -> Result<Vec<agentfile::File>> {
    let wanted = wanted_ids(ids)?;
    let mut by_id: BTreeMap<String, agentfile::File> = BTreeMap::new();
    for file in files {
        by_id.insert(file.id.clone(), file);
    }
    let missing: Vec<&str> = wanted
        .iter()
        .filter(|id| !by_id.contains_key(id.as_str()))
        .map(|id| id.as_str())
        .collect();
    if !missing.is_empty() {
        let intro = if missing.len() == 1 {
            "This agent is"
        } else {
            "These agents are"
        };
        return Err(anyhow!("{intro} not in the source folder: {}.", missing.join(", ")));
    }
    Ok(wanted.into_iter().filter_map(|id| by_id.remove(&id)).collect())
}

/// Copies the agents named in `ids` from `src` onto `dest`.
/// Other files in `dest` stay. When a copied agent already has a file at a different path, that file is removed.
pub(crate) fn copy_agent_ids(src: &str, dest: &str, ids: &[String]) -> Result<()> {
    let src = path::clean(src.trim());
    let dest = path::clean(dest.trim());
    if src.is_empty() || dest.is_empty() || !path::is_abs(&src) || !path::is_abs(&dest) {
        return Err(anyhow!("The agent folder is not available."));
    }
    if folders_overlap(&src, &dest) {
        return Err(anyhow!("The team folder cannot be the local agents folder."));
    }
    let chosen = keep(agentfile::list(&src)?, ids)?;
    let existing: HashMap<String, String> = agentfile::list(&dest)?
        .into_iter()
        .map(|file| (file.id, file.path))
        .collect();
    let mut staged: Vec<(String, String, Vec<u8>)> = Vec::with_capacity(chosen.len());
    for file in &chosen {
        let name = path::base(&file.path);
        if !one_segment(&file.category) || !one_segment(&name) {
            return Err(anyhow!("agent file path is not valid"));
        }
        let dest_path = path::join(&[&dest, &file.category, &name]);
        let body = std::fs::read(&file.path).map_err(|e| anyhow!("open {}: {e}", file.path))?;
        staged.push((file.id.clone(), dest_path, body));
    }
    mkdir_all(&dest, 0o755)?;
    for (id, dest_path, body) in &staged {
        write_agent_file(dest_path, body)?;
        if let Some(old) = existing.get(id) {
            remove_replaced(&dest, old, dest_path)?;
        }
    }
    Ok(())
}

/// Deletes the agent files in `dest` whose ids are in `ids`. Other files stay. An id that is already gone is skipped.
pub(crate) fn remove_agent_ids(dest: &str, ids: &[String]) -> Result<()> {
    let dest = path::clean(dest.trim());
    if dest.is_empty() || !path::is_abs(&dest) {
        return Err(anyhow!("The agent folder is not available."));
    }
    let wanted: BTreeSet<String> = ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    for file in agentfile::list(&dest)? {
        if !wanted.contains(&file.id) {
            continue;
        }
        match std::fs::remove_file(&file.path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(anyhow!("remove {}: {err}", file.path)),
        }
        remove_empty_dir(&path::dir(&file.path), &dest);
    }
    Ok(())
}

fn wanted_ids(ids: &[String]) -> Result<BTreeSet<String>> {
    let wanted: BTreeSet<String> = ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if wanted.is_empty() {
        return Err(anyhow!("Choose at least one agent to copy."));
    }
    Ok(wanted)
}

fn one_segment(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

fn folders_overlap(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    long == short || long.starts_with(&format!("{short}/"))
}

fn write_agent_file(dest: &str, body: &[u8]) -> Result<()> {
    mkdir_all(&path::dir(dest), 0o755)?;
    let tmp = format!("{dest}.kiki-tmp");
    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&tmp)
            .map_err(|e| anyhow!("open {tmp}: {e}"))?;
        file.write_all(body).map_err(|e| anyhow!("write {tmp}: {e}"))?;
        Ok(())
    })();
    if let Err(err) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("rename {tmp}: {err}"));
    }
    Ok(())
}

fn remove_replaced(root: &str, old_path: &str, new_path: &str) -> Result<()> {
    if path::clean(old_path) == path::clean(new_path) {
        return Ok(());
    }
    match std::fs::remove_file(old_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!("remove {old_path}: {e}")),
    }
    remove_empty_dir(&path::dir(old_path), root);
    Ok(())
}

fn remove_empty_dir(dir: &str, root: &str) {
    let dir = path::clean(dir);
    let root = path::clean(root);
    if dir == root || !dir.starts_with(&format!("{root}/")) {
        return;
    }
    match std::fs::read_dir(&dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return;
            }
        }
        Err(_) => return,
    }
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNER: &str = "---\nid: backend_runner\nname: Local Runner\ndescription: Local copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - backend\n---\n\nLocal.\n";
    const DOCUMENT: &str = "---\nid: requirement_document\nname: Document Writer\ndescription: Shared copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - document\n---\n\nFrom the app.\n";

    fn p(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn chosen_agents_are_copied_and_the_rest_stay() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        let dest = root.path().join("dest");
        std::fs::create_dir_all(src.join("backend")).unwrap();
        std::fs::create_dir_all(dest.join("requirement")).unwrap();
        std::fs::create_dir_all(dest.join("backend")).unwrap();
        let old_document = DOCUMENT.replace("From the app.", "Old copy.");
        std::fs::write(src.join("backend/requirement_document.md"), DOCUMENT).unwrap();
        std::fs::write(src.join("backend/backend_runner.md"), RUNNER).unwrap();
        std::fs::write(dest.join("requirement/requirement_document.md"), old_document).unwrap();
        std::fs::write(dest.join("backend/backend_runner.md"), "keep this runner").unwrap();
        std::fs::write(dest.join("backend/notes.txt"), "notes").unwrap();

        copy_agent_ids(&p(&src), &p(&dest), &["requirement_document".to_string()]).unwrap();

        let copied = std::fs::read_to_string(dest.join("backend/requirement_document.md")).unwrap();
        assert_eq!(copied, DOCUMENT);
        assert!(!dest.join("requirement/requirement_document.md").exists());
        assert!(!dest.join("requirement").exists());
        assert_eq!(
            std::fs::read_to_string(dest.join("backend/backend_runner.md")).unwrap(),
            "keep this runner"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("backend/notes.txt")).unwrap(),
            "notes"
        );
    }

    #[test]
    fn an_empty_choice_or_an_unknown_id_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        let dest = root.path().join("dest");
        std::fs::create_dir_all(src.join("backend")).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("backend/backend_runner.md"), RUNNER).unwrap();
        let empty = copy_agent_ids(&p(&src), &p(&dest), &[]).unwrap_err().to_string();
        assert!(empty.contains("at least one"), "empty = {empty}");
        let missing = copy_agent_ids(&p(&src), &p(&dest), &["missing".to_string()])
            .unwrap_err()
            .to_string();
        assert!(missing.contains("missing"), "missing = {missing}");
        assert!(!dest.join("backend").exists());
    }
}
