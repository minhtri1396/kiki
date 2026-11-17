//! The "Repositories for this agent" section.

use kiki_agentdef as agentdef;
use kiki_util::path;

use crate::snapshot::{Repo, Snapshot};

pub(super) fn write(b: &mut String, agent: &agentdef::Agent, snap: &Snapshot) {
    b.push_str("Repositories for this agent:\n");
    for item in &agent.repos {
        write_repo(b, item, snap);
    }
    b.push('\n');
}

fn write_repo(b: &mut String, item: &agentdef::Repo, snap: &Snapshot) {
    let kind = item.kind.trim();
    if kind.is_empty() {
        return;
    }
    let repo = snap.repo(kind);
    w!(b, "- {}\n", kind);
    write_folder_and_guide(b, &repo, &agentdef::guide_file(&item.guide));
    write_access(b, &repo, item.readonly);
    if kind != "document" {
        write_services(b, &repo);
    }
    write_note(b, &item.description);
}

fn write_folder_and_guide(b: &mut String, repo: &Repo, guide: &str) {
    if repo.folder.is_empty() {
        b.push_str("  folder: (not set)\n");
        w!(
            b,
            "  guide file: {} (read it and follow it whenever you work in this repository)\n",
            guide
        );
    } else {
        w!(b, "  folder: {}\n", repo.folder);
        w!(
            b,
            "  guide file: {} (read it and follow it whenever you work in this repository)\n",
            path::join(&[&repo.folder, guide])
        );
    }
}

/// The sparse checkout and readonly rules.
fn write_access(b: &mut String, repo: &Repo, readonly: bool) {
    if repo.sparse.is_empty() {
        if readonly {
            b.push_str("  Readonly: this agent can only read this repository. Do not create, edit, delete, rename, commit, or push.\n");
        }
        return;
    }
    w!(b, "  checkout: {}\n", repo.sparse);
    if readonly {
        w!(b, "  This repository is a sparse checkout of {}. The working tree contains that folder and files at the repository root. Other folders are not checked out. Do not disable sparse checkout or add another path. This repository is readonly for this agent. Read files only inside {}. Do not create, edit, delete, rename, commit, or push.\n", repo.sparse, repo.sparse);
    } else {
        w!(b, "  This repository is a sparse checkout of {}. The working tree contains that folder and files at the repository root. Other folders are not checked out. Do not disable sparse checkout or add another path. Read and change files only inside {}. Run git commit, git push, and git pull in the repository folder. Pull updates this checkout. A commit keeps every other path on the remote as it already is.\n", repo.sparse, repo.sparse);
    }
}

fn write_services(b: &mut String, repo: &Repo) {
    if repo.services.is_empty() {
        b.push_str("  services: (none)\n");
    } else {
        w!(b, "  services: {}\n", repo.services.join(", "));
    }
}

/// What the agent does in this repository, one indented line per line of the note.
fn write_note(b: &mut String, description: &str) {
    let note = description.trim();
    if note.is_empty() {
        return;
    }
    b.push_str("  Work in this repository:\n");
    for line in note.split('\n') {
        w!(b, "  {}\n", line);
    }
}
