//! Pattern: Adapter. `RuntimeTransport` is how a request reaches the Orca runtime. `SocketTransport` speaks to the
//! local app over its Unix socket; `FnTransport` adapts a plain closure, which is how tests inject a fake runtime.

use crate::wire::{nd, parse};
use anyhow::{anyhow, Result};
use kiki_util::{random_hex, Ctx};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

const NOT_RUNNING: &str = "Orca is not running. Open the Orca app and try again";
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Answers an Orca runtime method: `(ctx, method, params)`. `None` params means the method takes none.
pub type RuntimeFn = Arc<dyn Fn(&Ctx, &str, Option<&Value>) -> Result<Value> + Send + Sync>;

/// Sends one request to the Orca runtime and returns its `result`.
pub trait RuntimeTransport: Send + Sync {
    /// Calls `method` with `params`.
    fn call(&self, ctx: &Ctx, method: &str, params: Option<&Value>) -> Result<Value>;
}

/// Adapts a closure to the transport interface.
pub struct FnTransport(pub RuntimeFn);

impl RuntimeTransport for FnTransport {
    fn call(&self, ctx: &Ctx, method: &str, params: Option<&Value>) -> Result<Value> {
        (self.0)(ctx, method, params)
    }
}

/// The local Orca app, reached through the socket named in its runtime metadata file.
#[derive(Clone, Copy, Debug, Default)]
pub struct SocketTransport;

impl RuntimeTransport for SocketTransport {
    fn call(&self, ctx: &Ctx, method: &str, params: Option<&Value>) -> Result<Value> {
        let meta = read_runtime_metadata()?;
        let endpoint = meta.endpoint().ok_or_else(|| anyhow!(NOT_RUNNING))?;
        let mut payload = request_payload(&meta.auth_token, method, params)?;
        if ctx.done() {
            return Err(anyhow!(NOT_RUNNING));
        }
        let conn = UnixStream::connect(&endpoint).map_err(|_| anyhow!(NOT_RUNNING))?;
        let deadline = call_deadline(ctx);
        payload.push(b'\n');
        set_timeouts(&conn, deadline)?;
        (&conn).write_all(&payload).map_err(io_err)?;
        read_reply(&conn, deadline)
    }
}

/// The deadline of one call: 30 seconds, or sooner when the context ends sooner.
fn call_deadline(ctx: &Ctx) -> Instant {
    let deadline = Instant::now() + CALL_TIMEOUT;
    match ctx.remaining() {
        Some(left) => deadline.min(Instant::now() + left),
        None => deadline,
    }
}

fn request_payload(auth_token: &str, method: &str, params: Option<&Value>) -> Result<Vec<u8>> {
    let mut body = serde_json::Map::new();
    body.insert("id".into(), Value::String(random_hex(16)));
    body.insert("authToken".into(), Value::String(auth_token.to_string()));
    body.insert("method".into(), Value::String(method.to_string()));
    if let Some(p) = params {
        body.insert("params".into(), p.clone());
    }
    Ok(serde_json::to_vec(&Value::Object(body))?)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ReplyError {
    #[serde(deserialize_with = "nd")]
    message: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Frame {
    #[serde(rename = "_keepalive", deserialize_with = "nd")]
    keepalive: bool,
    #[serde(deserialize_with = "nd")]
    ok: bool,
    #[serde(deserialize_with = "nd")]
    error: ReplyError,
    result: Option<Value>,
}

/// Reads reply lines until the real answer, skipping keepalive frames.
fn read_reply(conn: &UnixStream, deadline: Instant) -> Result<Value> {
    let mut reader = BufReader::new(conn);
    loop {
        set_timeouts(conn, deadline)?;
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line).map_err(io_err)?;
        if n == 0 || !line.ends_with(b"\n") {
            return Err(anyhow!("EOF"));
        }
        let frame: Frame = parse(&line).map_err(|_| anyhow!("Orca returned an unreadable response"))?;
        if frame.keepalive {
            continue;
        }
        if !frame.ok {
            let mut message = frame.error.message;
            if message.is_empty() {
                message = "Orca rejected the request".to_string();
            }
            return Err(anyhow!("{message}"));
        }
        return Ok(frame.result.unwrap_or_else(|| json!({})));
    }
}

fn io_err(e: std::io::Error) -> anyhow::Error {
    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
        return anyhow!("i/o timeout");
    }
    anyhow!("{e}")
}

fn set_timeouts(conn: &UnixStream, deadline: Instant) -> Result<()> {
    let left = deadline.saturating_duration_since(Instant::now());
    // A zero timeout is rejected by the OS; an expired deadline fails fast instead.
    let left = left.max(Duration::from_millis(1));
    conn.set_read_timeout(Some(left))?;
    conn.set_write_timeout(Some(left))?;
    Ok(())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Transport {
    #[serde(deserialize_with = "nd")]
    kind: String,
    #[serde(deserialize_with = "nd")]
    endpoint: String,
}

impl Transport {
    fn is_local(&self) -> bool {
        self.kind == "unix" || self.kind == "named-pipe"
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RuntimeMetadata {
    #[serde(rename = "authToken", deserialize_with = "nd")]
    auth_token: String,
    #[serde(deserialize_with = "nd")]
    transports: Vec<Transport>,
    #[serde(deserialize_with = "nd")]
    transport: Transport,
}

impl RuntimeMetadata {
    /// The local socket endpoint, or `None` when the app is not reachable (no endpoint or no auth token).
    fn endpoint(&self) -> Option<String> {
        let mut endpoint = self
            .transports
            .iter()
            .find(|t| t.is_local())
            .map(|t| t.endpoint.clone())
            .unwrap_or_default();
        if endpoint.is_empty() && self.transport.is_local() {
            endpoint = self.transport.endpoint.clone();
        }
        (!endpoint.is_empty() && !self.auth_token.is_empty()).then_some(endpoint)
    }
}

fn read_runtime_metadata() -> Result<RuntimeMetadata> {
    let path = runtime_metadata_path()?;
    let raw = std::fs::read(path).map_err(|_| anyhow!(NOT_RUNNING))?;
    parse(&raw).map_err(|_| anyhow!(NOT_RUNNING))
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn runtime_metadata_path() -> Result<String> {
    if let Some(dir) = env_value("ORCA_USER_DATA_PATH") {
        return Ok(format!("{}/orca-runtime.json", dir.trim_end_matches('/')));
    }
    let home = env_value("HOME").ok_or_else(|| anyhow!(NOT_RUNNING))?;
    if cfg!(target_os = "macos") {
        Ok(format!("{home}/Library/Application Support/orca/orca-runtime.json"))
    } else if cfg!(target_os = "windows") {
        let app = env_value("APPDATA").ok_or_else(|| anyhow!(NOT_RUNNING))?;
        Ok(format!("{app}/orca/orca-runtime.json"))
    } else {
        let base = env_value("XDG_CONFIG_HOME").unwrap_or_else(|| format!("{home}/.config"));
        Ok(format!("{base}/orca/orca-runtime.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn fn_transport_forwards_the_call() {
        let transport = FnTransport(Arc::new(|_ctx, method, params| {
            Ok(json!({"method": method, "has": params.is_some()}))
        }));
        let got = transport.call(&Ctx::background(), "x.y", Some(&json!({}))).unwrap();
        assert_eq!(got, json!({"method": "x.y", "has": true}));
    }

    #[test]
    fn endpoint_prefers_listed_local_transports() {
        let meta: RuntimeMetadata = parse(
            br#"{"authToken":"t","transports":[{"kind":"tcp","endpoint":"h:1"},{"kind":"unix","endpoint":"/s"}],
                 "transport":{"kind":"unix","endpoint":"/old"}}"#,
        )
        .unwrap();
        assert_eq!(meta.endpoint().as_deref(), Some("/s"));
        let single: RuntimeMetadata =
            parse(br#"{"authToken":"t","transport":{"kind":"named-pipe","endpoint":"p"}}"#).unwrap();
        assert_eq!(single.endpoint().as_deref(), Some("p"));
        let no_token: RuntimeMetadata = parse(br#"{"transport":{"kind":"unix","endpoint":"/s"}}"#).unwrap();
        assert!(no_token.endpoint().is_none());
    }

    #[test]
    fn socket_transport_skips_keepalives_and_reports_errors() {
        // Unix socket paths are short on macOS, so the directory is under /tmp.
        let dir = tempfile::Builder::new().prefix("kk").tempdir_in("/tmp").unwrap();
        let sock = dir.path().join("s");
        let listener = UnixListener::bind(&sock).unwrap();
        let server = std::thread::spawn(move || {
            // Connections stay open until both replies are sent: macOS rejects socket options on a closed peer.
            let mut open = Vec::new();
            for reply in [
                "{\"_keepalive\":true}\n{\"ok\":true,\"result\":{\"a\":1}}\n",
                "{\"ok\":false,\"error\":{\"message\":\"nope\"}}\n",
            ] {
                let (conn, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&conn).read_line(&mut line).unwrap();
                assert!(
                    line.contains("\"method\":\"m\"") && line.contains("\"authToken\":\"tok\""),
                    "line = {line}"
                );
                (&conn).write_all(reply.as_bytes()).unwrap();
                open.push(conn);
            }
        });
        let meta = dir.path().join("orca-runtime.json");
        std::fs::write(
            &meta,
            format!(
                r#"{{"authToken":"tok","transport":{{"kind":"unix","endpoint":"{}"}}}}"#,
                sock.display()
            ),
        )
        .unwrap();
        let _guard = crate::tests::env_lock();
        std::env::set_var("ORCA_USER_DATA_PATH", dir.path());
        let first = SocketTransport.call(&Ctx::background(), "m", None);
        let second = SocketTransport.call(&Ctx::background(), "m", None);
        std::env::remove_var("ORCA_USER_DATA_PATH");
        server.join().unwrap();
        assert_eq!(first.unwrap(), json!({"a": 1}));
        assert_eq!(second.unwrap_err().to_string(), "nope");
    }
}
