//! Pattern: Adapter (dependency injection). [`System`] is the one seam to the machine: is a command installed,
//! and run `rtk`. Production uses [`RealSystem`]; tests inject a fake instead of replacing process globals.

use std::io::Read;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Result};

/// What running `rtk` produced: its combined stdout and stderr, and whether it succeeded.
pub struct RunOutput {
    pub output: Vec<u8>,
    pub result: Result<()>,
}

/// What the tools crate needs from the machine.
pub trait System: Send + Sync {
    /// Whether a CLI is on `PATH`.
    fn command_exists(&self, name: &str) -> bool;
    /// Runs `rtk` with `HOME` set to `home`.
    fn run_rtk(&self, home: &str, args: &[String]) -> RunOutput;
}

/// The real machine.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealSystem;

impl System for RealSystem {
    fn command_exists(&self, name: &str) -> bool {
        kiki_util::look_path(name).is_some()
    }

    fn run_rtk(&self, home: &str, args: &[String]) -> RunOutput {
        match run_combined("rtk", home, args) {
            Ok((output, result)) => RunOutput { output, result },
            Err(err) => RunOutput {
                output: Vec::new(),
                result: Err(err),
            },
        }
    }
}

/// Runs a program with stdout and stderr on one pipe, returning the combined output and the exit result.
fn run_combined(program: &str, home: &str, args: &[String]) -> Result<(Vec<u8>, Result<()>)> {
    let (mut reader, writer) = std::io::pipe()?;
    let writer_err = writer.try_clone()?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(writer_err);
    let mut child = cmd.spawn()?;
    // `cmd` holds the parent's copies of the write ends; drop them so the read below ends.
    drop(cmd);
    let mut out = Vec::new();
    let _ = reader.read_to_end(&mut out);
    let result = match child.wait() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(anyhow!("{status}")),
        Err(e) => Err(e.into()),
    };
    Ok((out, result))
}
