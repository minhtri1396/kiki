//! The "Repository indexes" section: how to search each repository's saved index.

use kiki_agentdef as agentdef;

use kiki_util::path;

use crate::snapshot::{Repo, Snapshot};

pub(super) fn write(b: &mut String, repos: &[agentdef::Repo], snap: &Snapshot) {
    let blocks: Vec<String> = repos.iter().filter_map(|item| index_block(item, snap)).collect();
    if blocks.is_empty() {
        return;
    }
    b.push_str("Repository indexes. For each repository this agent uses, read that repository's guide file, then use the index below. The index is the one saved for that repository. Do not set an index up yourself.\n\n");
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
    let guide = agentdef::guide_file(&item.guide);
    match repo.index.as_str() {
        "obsidian" => Some(obsidian_index(kind, &repo, &snap.root, &guide)),
        "codegraph" => Some(codegraph_index(kind, &repo, &snap.root, &guide)),
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

fn obsidian_index(kind: &str, repo: &Repo, root: &str, guide: &str) -> String {
    let path = folder_path(&repo.folder);
    let mut read =
        format!("The folder is not set. Do not invent a path. Read this repository's {guide} once the folder is set.");
    if path != NOT_SET {
        read = format!("Read {path}/{guide} before you search notes in this repository.");
    }
    let index_file = index_file_line(root, kind);
    format!(
        r##"### {kind} — Obsidian

Folder: {path}

{index_file}{read} Follow that file for this repository. If it still tells you to use GitNexus, ignore that and follow this index.

The vault name is the last segment of the folder path, not the absolute path. Obsidian is set up when {path}/.obsidian exists and this command, run in the folder, prints the folder:

```
obsidian vault=<name> vault info=path
```

vault=<name> is the first parameter. The vault is open in Obsidian. The CLI is the Obsidian CLI: Obsidian 1.12 or newer, with Settings → General → Command line interface enabled. The app must be running. The first command launches it if it is not.

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

fn codegraph_index(kind: &str, repo: &Repo, root: &str, guide: &str) -> String {
    let path = folder_path(&repo.folder);
    let mut services = "(none)".to_string();
    if !repo.services.is_empty() {
        services = repo.services.join(", ");
    }
    let mut read = format!(
        "The folder is not set. Do not invent a path. Read this repository's {guide}, and each service's {guide}, once the folder is set."
    );
    if path != NOT_SET {
        read = format!(
            "Before you read code, read {path}/{guide}. For each service this work touches, also read {path}/<project root>/{guide}."
        );
    }
    let index_file = index_file_line(root, kind);
    format!(
        r##"### {kind} — CodeGraph

Folder: {path}

{index_file}Services: {services}

{read} A service is an Nx project from the services list. Find its root in project.json (`npx nx show projects`, run in the folder). If the project root is ".", that file is the folder file; read it once. The folder file covers the repository. The service file covers the files in that service. When they conflict inside a service, follow the service file. If {guide} is missing, ask the user once and stop. Do not invent the structure or the rules.

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

Open a file only when the result marks it stale, when the graph does not cover it (schemas, generated code, fixtures, config, assets), or when {guide} tells you to. Do not read the whole repository. .codegraph/ is gitignored; leave it uncommitted.
"##
    )
}
