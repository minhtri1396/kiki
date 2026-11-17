//! The files and folders KIKI writes next to an app folder.

use kiki_util::path;

/// Entry is a file or directory the app writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub dir: bool,
}

impl Entry {
    fn file(path: String) -> Entry {
        Entry { path, dir: false }
    }

    fn dir(path: String) -> Entry {
        Entry { path, dir: true }
    }

    /// An entry for `name` inside `parent`.
    pub(crate) fn inside(parent: &str, name: &str) -> Entry {
        Entry::dir(path::join(&[parent, name]))
    }
}

/// The settings files KIKI keeps next to the setup file, each with its temporary copy.
const SETTINGS_FILES: [&str; 3] = ["mcp.yaml", "tools.yaml", "atlassian.yaml"];

/// AppEntries lists the files and folders KIKI writes for this app folder.
/// agent_dir is included when it is set. Repo folders are always included.
pub fn app_entries(root: &str, agent_dir: &str, setup_path: &str, log_path: &str) -> Vec<Entry> {
    let setup_dir = path::dir(setup_path);
    let mut entries = vec![
        Entry::file(setup_path.to_string()),
        Entry::file(format!("{setup_path}.tmp")),
    ];
    for name in SETTINGS_FILES {
        let file = path::join(&[&setup_dir, name]);
        entries.push(Entry::file(file.clone()));
        entries.push(Entry::file(format!("{file}.tmp")));
    }
    entries.push(Entry::file(log_path.to_string()));
    for suffix in ["-shm", "-wal", "-journal"] {
        entries.push(Entry::file(format!("{log_path}{suffix}")));
    }
    for kind in kiki_toolchain::KINDS {
        entries.push(Entry::dir(kiki_toolchain::folder(root, kind)));
    }
    if !agent_dir.trim().is_empty() {
        entries.push(Entry::dir(agent_dir.to_string()));
    }
    entries
}
