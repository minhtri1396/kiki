//! The `kiki guardrail <tool>` command: read the hook payload, ask the running app, print the tool's reply.

mod inspect;
mod post;
mod reduce;
#[cfg(test)]
mod tests;

use std::io::{Read, Write};

use kiki_aitool::AiTool;
use kiki_util::json::marshal;
use kiki_util::path as fpath;
use serde_json::{json, Value};

use crate::endpoint::load;
use crate::reply::reply;
use inspect::{find_owner, first_owned, inspect};
pub use reduce::reduce;

/// The `kiki guardrail <tool>` command. AI tools start it before input and output.
/// It reads the hook payload from stdin and prints the reply to stdout. An id that is not a known AI tool gets
/// the Cursor reply format.
pub fn run(tool: &str) -> i32 {
    let tool = AiTool::parse_loose(tool).unwrap_or(AiTool::Cursor);
    let mut raw = Vec::new();
    let _ = std::io::stdin().lock().take(1 << 20).read_to_end(&mut raw);
    let mut stdout = std::io::stdout().lock();
    let home = match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => home,
        _ => {
            let _ = write!(stdout, "{}", reply(tool, "", true, ""));
            let _ = stdout.flush();
            return 0;
        }
    };
    let agent = std::env::var("KIKI_AGENT").unwrap_or_default();
    let _ = write!(stdout, "{}", gate(&home, tool, &raw, &agent, post::post_guard));
    let _ = stdout.flush();
    0
}

/// Asks the running app about this hook. A folder that is not a KIKI repository is allowed.
/// When the app is not running, the action is allowed. The app decides only while it is up.
///
/// `post` sends the request body to the app at the given base URL and returns `(allow, message)`;
/// production uses an HTTP call, tests pass a closure.
pub fn gate(
    home: impl AsRef<std::path::Path>,
    tool: AiTool,
    raw: &[u8],
    agent: &str,
    post: impl Fn(&str, &[u8]) -> anyhow::Result<(bool, String)>,
) -> String {
    let input = inspect(raw);
    let event = input.event.as_str();
    let eps = match load(home) {
        Ok(eps) if !eps.is_empty() => eps,
        _ => return reply(tool, event, true, ""),
    };
    let Some(ep) = find_owner(&eps, &input.cwd, &input.paths) else {
        return reply(tool, event, true, "");
    };
    let cwd = if input.cwd.is_empty() {
        fpath::dir(&first_owned(ep, &input.paths))
    } else {
        input.cwd.clone()
    };
    let payload = marshal(&json!({
        "tool": tool.id(),
        "agent": agent,
        "cwd": cwd,
        "event": event,
        "input": serde_json::from_slice(&reduce(raw)).unwrap_or(Value::Null),
    }));
    match post(&ep.url, payload.as_bytes()) {
        Ok((allow, message)) => reply(tool, event, allow, &message),
        Err(_) => reply(tool, event, true, ""),
    }
}
