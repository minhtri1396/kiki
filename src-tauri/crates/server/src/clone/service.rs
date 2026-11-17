//! The clone use case: check the request, prepare the folder, clone, remember the sparse folder.

use std::path::Path;
use std::time::Duration;

use kiki_console as console;
use kiki_toolchain as toolchain;

use super::folder::{folder_empty, mkdir_all};
use super::git::git_clone;
use super::url::valid_git_url;
use crate::api_error::{ApiError, ApiResult, OrStatus};
use crate::Server;
use kiki_util::Ctx;

/// How long a clone may take.
const CLONE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

impl Server {
    /// Clones `url` (limited to the `sparse` folder when given) into the folder of repository `kind`, and saves
    /// the sparse folder in the setup. The folder must be empty.
    pub(crate) fn clone_repository(&self, ctx: &Ctx, kind: &str, url: &str, sparse: &str) -> ApiResult<()> {
        let remote = url.trim();
        if !valid_git_url(remote) {
            return Err(ApiError::new(400, "Git URL is not valid"));
        }
        let sparse = console::sparse_path(sparse).or_status(400)?;
        let folder = self.clone_target(kind)?;
        let ctx = ctx.with_timeout(CLONE_TIMEOUT);
        git_clone(&ctx, remote, &folder, &sparse).or_status(400)?;
        let snap = self.load().or_status(500)?;
        self.store.save_repo_sparse(&snap, kind, &sparse).or_status(500)
    }

    /// The folder a repository kind clones into, created when missing. It must hold no repository content yet.
    fn clone_target(&self, kind: &str) -> ApiResult<String> {
        let snap = self.load().or_status(500)?;
        let mut folder = snap.repo(kind).folder.trim().to_string();
        if folder.is_empty() {
            folder = toolchain::folder(&self.root, kind);
        }
        mkdir_all(Path::new(&folder)).map_err(|err| ApiError::new(500, err))?;
        if !folder_empty(&folder).or_status(500)? {
            return Err(ApiError::new(400, "folder is not empty"));
        }
        Ok(folder)
    }
}
