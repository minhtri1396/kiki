//! The CodeGraph strategy: `codegraph init` as a background job, `codegraph status` as the check.

use super::{IndexTool, Launch, Setup};
use crate::checks::{command_missing, evaluate, is_dir};
use crate::env::Env;
use crate::model::Check;
use crate::process::combined_output;
use anyhow::{anyhow, Result};
use kiki_util::{path, Ctx};
use std::time::Duration;

/// The phrase `codegraph status` prints before `init` has run.
const NOT_INITIALIZED: &str = "not initialized";

pub(super) struct Codegraph;

impl IndexTool for Codegraph {
    fn name(&self) -> &'static str {
        "codegraph"
    }

    fn dir_name(&self) -> &'static str {
        ".codegraph"
    }

    fn prepare(&self, _env: &Env, req: &Setup<'_>) -> Result<Launch> {
        if !is_dir(req.folder) {
            return Err(anyhow!("folder {} does not exist", req.folder));
        }
        let ignore_path = kiki_gitignore::index(req.folder, &[".codegraph", ".obsidian"])?;
        let note = if ignore_path.is_empty() {
            String::new()
        } else {
            format!("Git ignore: {ignore_path}")
        };
        Ok(Launch {
            dir: req.root.to_string(),
            args: kiki_toolchain::codegraph_init(req.folder),
            note,
        })
    }

    fn check(&self, env: &Env, ctx: &Ctx, root: &str, folder: &str) -> Check {
        if folder.is_empty() {
            return Check::not_ready("folder is not set");
        }
        let exists = is_dir(&path::join(&[folder, ".codegraph"]));
        let args = kiki_toolchain::codegraph_status(folder);
        let bin = match (env.look_path)(&args[0]) {
            Ok(bin) => bin,
            Err(_) => return command_missing(exists, &args[0]),
        };
        let cctx = ctx.with_timeout(Duration::from_secs(30));
        let (out, run_err) = combined_output(&cctx, &bin, &args[1..], root);
        evaluate(exists, &out, run_err.as_deref(), NOT_INITIALIZED)
    }
}
