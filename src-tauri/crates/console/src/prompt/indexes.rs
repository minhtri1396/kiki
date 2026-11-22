//! The "Repository indexes" section: how to search each repository's saved index.

use kiki_agentdef as agentdef;

use kiki_util::path;

use crate::snapshot::{Repo, Snapshot};

pub(super) fn write(b: &mut String, repos: &[agentdef::Repo], snap: &Snapshot) {
    let blocks: Vec<String> = repos.iter().filter_map(|item| index_block(item, snap)).collect();
    if blocks.is_empty() {
        return;
    }
    b.push_str("Repository indexes. For each repository this agent uses, read that repository's guide files, then use the index below. A service uses its own guide files when they are set, and the repository guide files when it shares them. The index is the one saved for that repository. Do not set an index up yourself.\n\n");
    for block in &blocks {
        b.push_str(block);
        if !block.ends_with('\n') {
            b.push('\n');
        }
        b.push('\n');
    }
}

/// The index text for one repository, or `None` when no index is saved for it.
fn index_block(item: &agentdef::Repo, snap: &Snapshot) -> Option<String> {
    let kind = item.kind.trim();
    let repo = snap.repo(kind);
    match repo.index.as_str() {
        "obsidian" => Some(obsidian_index(kind, &repo, &snap.root)),
        "codegraph" => Some(codegraph_index(kind, &repo, &snap.root)),
        _ => None,
    }
}

/// What the prompt shows for a repository whose folder is empty.
const NOT_SET: &str = "(not set)";

fn folder_path(folder: &str) -> String {
    let folder = folder.trim();
    if folder.is_empty() {
        return NOT_SET.to_string();
    }
    folder.to_string()
}

fn index_file_line(root: &str, kind: &str) -> String {
    let root = root.trim();
    if root.is_empty() {
        return String::new();
    }
    format!(
        "Index file: {}\n\n",
        path::join(&[root, ".kiki", "repos", &format!("{kind}.yaml")])
    )
}

fn obsidian_index(kind: &str, repo: &Repo, root: &str) -> String {
    let path = folder_path(&repo.folder);
    let read = if repo.guides.is_empty() {
        "No guide files are set for this repository. Do not invent one.".to_string()
    } else if path == NOT_SET {
        format!(
            "The folder is not set. Do not invent a path. Read {} once the folder is set.",
            super::repos::guide_phrase(repo, &repo.guides)
        )
    } else {
        format!(
            "Read {} before you search notes in this repository.",
            super::repos::guide_phrase(repo, &repo.guides)
        )
    };
    let index_file = index_file_line(root, kind);
    format!(
        r##"### {kind} — Obsidian

Folder: {path}

{index_file}{read} Follow the guide files for this repository when they are set. If a guide file still tells you to use GitNexus, ignore that and follow this index.

The vault name is the last segment of the folder path, not the absolute path. Obsidian is set up when {path}/.obsidian exists and `obsidian vault info=path` prints this folder. When only one vault uses that name, pass it as the first parameter so the CLI reads this vault and not another open one:

```
obsidian vault=<name> vault info=path
```

When more than one vault uses that name, do not pass vault=<name>. That command does not return. Run the commands below without it. The vault is open in Obsidian. The CLI is the Obsidian CLI: Obsidian 1.12 or newer, with Settings → General → Command line interface enabled. The app must be running. The first command launches it if it is not.

Search and read. Do not list the folder. Use a path the task already names. In a document repository that is path=ui-system-design for the UI system, or the area folder for requirements.

```
obsidian vault=<name> search query="<words>" limit=20
obsidian vault=<name> search:context query="<words>" path="<folder>" limit=20
obsidian vault=<name> read path="<path from the vault root>"
obsidian vault=<name> backlinks path="<path>"
obsidian vault=<name> links path="<path>"
```

Open a file only when search, backlinks, or links point to it, or when the task already names the path.

If it is not set up, do not create the vault. Ask the user once: Read files (open only a path you already know, and say this index was not used) or Wait for setup (stop until the user sets up Obsidian on this folder in KIKI). Do not choose for the user. If the search returns nothing useful, ask the user. Do not walk the folder unless the user chose Read files.
"##
    )
}

fn codegraph_index(kind: &str, repo: &Repo, root: &str) -> String {
    let path = folder_path(&repo.folder);
    let divided = repo.uses_services();
    let services = if repo.services.is_empty() {
        "(none)".to_string()
    } else {
        repo.services.join(", ")
    };
    let services_line = if divided {
        format!("Services: {services}\n\n")
    } else {
        String::new()
    };
    let read = codegraph_read(repo, path == NOT_SET);
    let service_notes = service_guide_notes(repo);
    let missing = if listed_guide(repo) {
        " If a listed guide file is missing, ask the user once and stop. Do not invent the structure or the rules."
    } else {
        ""
    };
    let index_file = index_file_line(root, kind);
    format!(
        r##"### {kind} — CodeGraph

Folder: {path}

{index_file}{services_line}{read}{service_notes}{missing}

CodeGraph is set up when {path}/.codegraph exists and `codegraph status {path}` runs without a "not initialized" error. Do not run `codegraph init` yourself, and do not install CodeGraph. If it is not set up, ask the user once: Read files (search the exact symbol or path and read only the files you need, and say this index was not used) or Wait for setup (stop until the user sets up CodeGraph on this folder in KIKI). Do not choose for the user.

Query this index before grep, find, or reading the repository.

```
codegraph explore -p {path} "<question>"
codegraph callers -p {path} <symbol>
codegraph callees -p {path} <symbol>
codegraph impact -p {path} <symbol>
codegraph affected -p {path} <files>
codegraph sync {path}
```

MCP: codegraph_explore with projectPath set to the folder, and the question in query. Run codegraph sync when codegraph status shows pending changes.

Open a file only when the result marks it stale, when the graph does not cover it (schemas, generated code, fixtures, config, assets), or when a guide file tells you to. Do not read the whole repository. .codegraph/ is gitignored; leave it uncommitted.
"##
    )
}

fn listed_guide(repo: &Repo) -> bool {
    if !repo.guides.is_empty() {
        return true;
    }
    repo.service_guides
        .iter()
        .any(|row| !row.shared && row.guides.iter().any(|name| !name.trim().is_empty()))
}

fn codegraph_read(repo: &Repo, folder_missing: bool) -> String {
    if repo.guides.is_empty() {
        return "No guide files are set for this repository. Do not invent one.".to_string();
    }
    let listed = super::repos::guide_phrase(repo, &repo.guides);
    if folder_missing {
        format!("The folder is not set. Do not invent a path. Read {listed} once the folder is set.")
    } else {
        format!("Before you read code, read {listed}.")
    }
}

fn service_guide_notes(repo: &Repo) -> String {
    if !repo.uses_services() || repo.services.is_empty() {
        return String::new();
    }
    let mut text = String::new();
    for service in &repo.services {
        text.push(' ');
        match repo.service_guide_list(service) {
            None => text.push_str(&format!("For service {service}, use this repository's guide files.")),
            Some([]) => {
                text.push_str(&format!(
                    "For service {service}, no guide files are set. Do not invent one."
                ));
            }
            Some(names) => {
                let listed = super::repos::guide_phrase(repo, names);
                text.push_str(&format!(
                    "For service {service}, read {listed}. When those files conflict with the repository guide files inside {service}, follow the service files."
                ));
            }
        }
    }
    text
}
