//! Fills the agent document with the paths of this launch. The root is the absolute path of the
//! folder the app is running in.

use crate::snapshot::Snapshot;

/// Replaces the root, setup file, and workspace placeholders with this launch's values.
pub(super) fn fill(body: &str, snap: &Snapshot) -> String {
    let mut text = body.to_string();
    let root = snap.root.trim();
    if !root.is_empty() {
        text = text.replace("the `Folder root:` line in the launch prompt", root);
        text = text.replace(" Do not assume a fixed absolute path.", "");
    }
    let setup = snap.setup_file.trim();
    if !setup.is_empty() {
        text = text.replace(
            "its absolute path is the `Setup file:` line in the launch prompt",
            setup,
        );
    }
    let workspace = snap.workspace_id.trim();
    if !workspace.is_empty() {
        text = text.replace("<ws>", workspace);
    }
    text
}
