use std::io::Read;
use std::time::Duration;

use anyhow::{anyhow, Result};
use base64::Engine;
use kiki_util::Ctx;

use super::{HttpRequest, HttpResponse, HttpTransport};

/// Longest wait for one request when the context has no earlier deadline.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Most response bytes read.
const MAX_BODY: u64 = 1 << 20;

/// The real network: `ureq` with basic auth.
///
/// The per-request timeout is 20 seconds or what is left of the context, whichever is shorter,
/// and at most 1 MiB of the body is read.
#[derive(Debug, Default, Clone, Copy)]
pub struct UreqTransport;

impl HttpTransport for UreqTransport {
    fn send(&self, ctx: &Ctx, request: &HttpRequest<'_>) -> Result<HttpResponse> {
        let verb = request.method.verb();
        let url = request.url;
        if let Some(err) = ctx.err() {
            return Err(anyhow!("{verb} \"{url}\": {err}"));
        }
        let mut timeout = REQUEST_TIMEOUT;
        if let Some(left) = ctx.remaining() {
            timeout = timeout.min(left);
        }
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", request.user, request.token));
        let mut req = agent
            .request(request.method.as_str(), url)
            .set("Authorization", &format!("Basic {auth}"))
            .set("Accept", "application/json");
        let result = match request.payload {
            Some(body) => {
                req = req.set("Content-Type", "application/json");
                req.send_bytes(body)
            }
            None => req.call(),
        };
        let res = match result {
            Ok(res) => res,
            Err(ureq::Error::Status(_, res)) => res,
            Err(err) => return Err(anyhow!("{verb} \"{url}\": {err}")),
        };
        let status = res.status();
        let mut body = Vec::new();
        res.into_reader()
            .take(MAX_BODY)
            .read_to_end(&mut body)
            .map_err(|err| anyhow!("{verb} \"{url}\": {err}"))?;
        Ok(HttpResponse { status, body })
    }
}
