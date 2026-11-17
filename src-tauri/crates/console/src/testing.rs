//! A fake `CliHost` for tests.

use std::io::Cursor;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use kiki_util::Ctx;

use crate::host::{CliHost, CommandRun};

type ExistsFn = dyn Fn(&str) -> bool + Send + Sync;
type StartFn = dyn Fn(&Ctx, &str, &str, &[String]) -> Result<CommandRun> + Send + Sync;

/// Every CLI is installed and starting one fails, until the test says otherwise.
pub(crate) struct FakeHost {
    exists: Arc<ExistsFn>,
    start: Arc<StartFn>,
}

impl FakeHost {
    pub(crate) fn new() -> FakeHost {
        FakeHost {
            exists: Arc::new(|_| true),
            start: Arc::new(|_, _, _, _| Err(anyhow!("no process in tests"))),
        }
    }

    /// Only the named CLIs are installed.
    pub(crate) fn installed(mut self, names: &'static [&'static str]) -> FakeHost {
        self.exists = Arc::new(move |name| names.contains(&name));
        self
    }

    /// Starting a CLI calls `start`.
    pub(crate) fn starting(
        mut self,
        start: impl Fn(&Ctx, &str, &str, &[String]) -> Result<CommandRun> + Send + Sync + 'static,
    ) -> FakeHost {
        self.start = Arc::new(start);
        self
    }

    /// Starting a CLI prints `stdout` and `stderr` and exits cleanly.
    pub(crate) fn printing(self, stdout: &str, stderr: &str) -> FakeHost {
        let (stdout, stderr) = (stdout.to_string(), stderr.to_string());
        self.starting(move |_, _, _, _| Ok(fake_run(&stdout, &stderr)))
    }
}

impl CliHost for FakeHost {
    fn command_exists(&self, name: &str) -> bool {
        (self.exists)(name)
    }

    fn start(&self, ctx: &Ctx, dir: &str, bin: &str, args: &[String]) -> Result<CommandRun> {
        (self.start)(ctx, dir, bin, args)
    }
}

/// A CLI run that prints the given text and exits cleanly.
pub(crate) fn fake_run(stdout: &str, stderr: &str) -> CommandRun {
    CommandRun {
        stdout: Some(Box::new(Cursor::new(stdout.as_bytes().to_vec()))),
        stderr: Some(Box::new(Cursor::new(stderr.as_bytes().to_vec()))),
        wait: Box::new(|| Ok(())),
    }
}
