//! The "Repositories for this agent" section.

use kiki_agentdef as agentdef;

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
    write_folder(b, &repo);
    write_guides(b, &repo);
    write_access(b, &repo, item.readonly);
    write_services(b, &repo);
    write_note(b, &item.description);
}

fn write_folder(b: &mut String, repo: &Repo) {
    if repo.folder.is_empty() {
        b.push_str("  folder: (not set)\n");
    } else {
        w!(b, "  folder: {}\n", repo.folder);
    }
}

fn write_guides(b: &mut String, repo: &Repo) {
    if repo.guides.is_empty() {
        b.push_str("  guide files: (none)\n");
        b.push_str("  No guide files are set for this repository.\n");
        return;
    }
    b.push_str("  guide files:\n");
    for name in &repo.guides {
        w!(b, "    - {}\n", repo.guide_location(name));
    }
    b.push_str("  Read these guide files and follow them whenever you work in this repository.\n");
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
    if !repo.uses_services() {
        return;
    }
    if repo.services.is_empty() {
        b.push_str("  services: (none)\n");
        b.push_str("  This repository is divided into Nx services. None are selected, so do not use a service.\n");
        return;
    }
    w!(b, "  services: {}\n", repo.services.join(", "));
    b.push_str("  These are the Nx services this agent may use in this repository.\n");
    for service in &repo.services {
        write_service_guides(b, repo, service);
    }
}

fn write_service_guides(b: &mut String, repo: &Repo, service: &str) {
    match repo.service_guide_list(service) {
        None => w!(b, "  service {service}: use this repository's guide files.\n"),
        Some([]) => {
            w!(b, "  service {service} guide files: (none)\n");
            w!(b, "  No guide files are set for {service}.\n");
        }
        Some(names) => {
            w!(b, "  service {service} guide files:\n");
            for name in names {
                w!(b, "    - {}\n", repo.guide_location(name));
            }
            w!(
                b,
                "  Read {service}'s guide files and follow them when you work in that service. When they conflict with the repository guide files inside {service}, follow the service files.\n"
            );
        }
    }
}

/// Joins guide locations the way a sentence lists them.
pub(super) fn guide_phrase(repo: &Repo, names: &[String]) -> String {
    let paths: Vec<String> = names.iter().map(|name| repo.guide_location(name)).collect();
    match paths.len() {
        0 => String::new(),
        1 => paths[0].clone(),
        2 => format!("{} and {}", paths[0], paths[1]),
        _ => {
            let (last, rest) = paths.split_last().unwrap();
            format!("{}, and {last}", rest.join(", "))
        }
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
