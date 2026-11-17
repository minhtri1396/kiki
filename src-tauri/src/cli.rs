//! Command line: `kiki [--addr HOST:PORT] [--no-open] [--root DIR]` or `kiki guardrail [TOOL]`.
//!
//! Like Go's `flag` package, flags accept `-name`, `--name`, `-name=value`, and `-name value`.
//! Pattern: Command. Each form of the command line parses into one value that `run` executes.

use anyhow::{anyhow, Result};

pub const DEFAULT_ADDR: &str = "127.0.0.1:8787";

/// What the program was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Cli {
    /// `kiki guardrail [TOOL]`: the hook that Cursor, Claude, Codex, and Gemini run before each action.
    Guardrail { tool: String },
    /// Start the app (window plus the loopback API).
    Serve(Options),
}

/// Options of the app command.
#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    /// Loopback listen address of the HTTP adapter.
    pub addr: String,
    /// Serve the address without opening a window.
    pub no_open: bool,
    /// Directory that holds `.kiki` and the repository folders. Default: beside the executable.
    pub root: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            addr: DEFAULT_ADDR.to_string(),
            no_open: false,
            root: None,
        }
    }
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli> {
        let args: Vec<String> = args.into_iter().collect();
        if args.first().map(String::as_str) == Some("guardrail") {
            let tool = args.get(1).cloned().unwrap_or_else(|| "cursor".to_string());
            return Ok(Cli::Guardrail { tool });
        }
        let mut options = Options::default();
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            let name = arg.trim_start_matches('-');
            if !arg.starts_with('-') || name.is_empty() {
                break;
            }
            let (key, inline) = match name.split_once('=') {
                Some((key, value)) => (key, Some(value.to_string())),
                None => (name, None),
            };
            match key {
                "addr" => options.addr = value(&args, &mut index, inline, "addr")?,
                "root" => options.root = Some(value(&args, &mut index, inline, "root")?),
                "no-open" => options.no_open = flag(inline, "no-open")?,
                other => return Err(anyhow!("flag provided but not defined: -{other}")),
            }
            index += 1;
        }
        Ok(Cli::Serve(options))
    }
}

fn value(args: &[String], index: &mut usize, inline: Option<String>, name: &str) -> Result<String> {
    if let Some(value) = inline {
        return Ok(value);
    }
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| anyhow!("flag needs an argument: -{name}"))
}

fn flag(inline: Option<String>, name: &str) -> Result<bool> {
    match inline.as_deref() {
        None | Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(other) => Err(anyhow!("invalid boolean value {other:?} for -{name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli> {
        Cli::parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn defaults() {
        assert_eq!(parse(&[]).unwrap(), Cli::Serve(Options::default()));
    }

    #[test]
    fn flags_in_every_spelling() {
        let expected = Cli::Serve(Options {
            addr: "127.0.0.1:9".into(),
            no_open: true,
            root: Some("/r".into()),
        });
        assert_eq!(
            parse(&["-addr", "127.0.0.1:9", "--no-open", "--root=/r"]).unwrap(),
            expected
        );
        assert_eq!(
            parse(&["--addr=127.0.0.1:9", "-no-open=true", "-root", "/r"]).unwrap(),
            expected
        );
    }

    #[test]
    fn guardrail_defaults_to_cursor() {
        assert_eq!(parse(&["guardrail"]).unwrap(), Cli::Guardrail { tool: "cursor".into() });
        assert_eq!(
            parse(&["guardrail", "claude"]).unwrap(),
            Cli::Guardrail { tool: "claude".into() }
        );
    }

    #[test]
    fn bad_flags_are_errors() {
        assert!(parse(&["-nope"]).is_err());
        assert!(parse(&["-addr"]).is_err());
        assert!(parse(&["-no-open=maybe"]).is_err());
    }
}
