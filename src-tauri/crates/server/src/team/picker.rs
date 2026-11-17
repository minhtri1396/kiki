//! Pattern: Strategy (dependency injection). The folder dialog is a value the server owns, so a test hands
//! it a closure and production uses the system dialog; no process-wide hook is involved.

use std::sync::Arc;

use anyhow::{anyhow, Result};

type PickFn = dyn Fn() -> Result<String> + Send + Sync;

/// Asks the user to choose a folder. Cloning is cheap: clones share the same strategy.
#[derive(Clone)]
pub struct FolderPicker {
    pick: Arc<PickFn>,
}

impl FolderPicker {
    /// A picker that answers with `pick` (used by tests and by hosts that bring their own dialog).
    pub fn new(pick: impl Fn() -> Result<String> + Send + Sync + 'static) -> FolderPicker {
        FolderPicker { pick: Arc::new(pick) }
    }

    /// The operating system's folder dialog.
    pub fn system() -> FolderPicker {
        FolderPicker::new(system_choose_folder)
    }

    /// Opens the dialog and returns the chosen path.
    pub fn pick(&self) -> Result<String> {
        (self.pick)()
    }
}

impl Default for FolderPicker {
    fn default() -> FolderPicker {
        FolderPicker::system()
    }
}

fn system_choose_folder() -> Result<String> {
    if !cfg!(target_os = "macos") {
        return Err(anyhow!("Enter the full path of the team folder."));
    }
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(r#"POSIX path of (choose folder with prompt "Choose the team folder")"#)
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => return Err(anyhow!("Folder selection was cancelled.")),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let path = text.trim().trim_end_matches('/').to_string();
    if path.is_empty() {
        return Err(anyhow!("Folder selection was cancelled."));
    }
    Ok(path)
}
