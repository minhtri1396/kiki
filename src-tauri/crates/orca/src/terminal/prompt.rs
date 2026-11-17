//! The launch prompt: after a launch, wait until the agent has taken it, pressing Enter if it stayed in the input.

use super::{TerminalView, Terminals};
use anyhow::Result;
use kiki_util::Ctx;
use std::time::{Duration, Instant};

/// How long to watch for a prompt left in the input.
const PROMPT_SUBMIT_WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(400);

impl Terminals {
    /// Presses Enter when the launch left the prompt in the agent's input.
    pub(crate) fn submit_pasted_prompt(&self, ctx: &Ctx, handle: &str) -> Result<()> {
        let deadline = Instant::now() + PROMPT_SUBMIT_WAIT;
        loop {
            if let Ok(view) = self.read(ctx, handle) {
                if unsent_prompt(&view) {
                    if self.agent_is_busy(ctx, handle) && Instant::now() < deadline && ctx.err().is_none() {
                        wait_prompt(ctx, POLL)?;
                        continue;
                    }
                    return self.press_enter(ctx, handle);
                }
            }
            if let Some(e) = ctx.err() {
                return Err(anyhow::Error::new(e));
            }
            if Instant::now() > deadline {
                return Ok(());
            }
            wait_prompt(ctx, POLL)?;
        }
    }

    /// True when the terminal reports that it is still updating.
    fn agent_is_busy(&self, ctx: &Ctx, handle: &str) -> bool {
        matches!(self.idle(ctx, handle), Ok(false))
    }
}

/// Whether the screen still shows the prompt in the input (a draft, or the "pasted text" placeholder).
pub(crate) fn unsent_prompt(view: &TerminalView) -> bool {
    if !view.draft.trim().is_empty() {
        return true;
    }
    view.log.to_lowercase().contains("pasted text")
}

/// Sleeps `d`, or fails as soon as `ctx` finishes.
fn wait_prompt(ctx: &Ctx, d: Duration) -> Result<()> {
    let end = Instant::now() + d;
    loop {
        if let Some(e) = ctx.err() {
            return Err(anyhow::Error::new(e));
        }
        let now = Instant::now();
        if now >= end {
            return Ok(());
        }
        std::thread::sleep((end - now).min(Duration::from_millis(10)));
    }
}
