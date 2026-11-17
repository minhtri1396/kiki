//! The HTTP call from a hook to the running app.

use std::time::Duration;

use anyhow::{anyhow, bail};
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct GuardReply {
    #[serde(default)]
    allow: bool,
    #[serde(default)]
    message: String,
}

/// Posts the request body to the app at `base` and returns its `(allow, message)` decision.
pub(super) fn post_guard(base: &str, body: &[u8]) -> anyhow::Result<(bool, String)> {
    let url = format!("{}/api/guardrail", base.trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build();
    let res = match agent
        .post(&url)
        .set("Content-Type", "application/json")
        .send_bytes(body)
    {
        Ok(res) => res,
        Err(ureq::Error::Status(code, _)) => bail!("status {code}"),
        Err(e) => return Err(anyhow!("{e}")),
    };
    if res.status() != 200 {
        bail!("status {}", res.status());
    }
    let mut de = serde_json::Deserializer::from_reader(res.into_reader());
    let payload = GuardReply::deserialize(&mut de)?;
    Ok((payload.allow, payload.message))
}
