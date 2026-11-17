//! Pattern: Adapter. A minimal HTTP layer between `tiny_http` (or the Tauri IPC) and the console API.
//!
//! * [`Request`] / [`Response`] are plain values, so handlers (and tests) never need a socket:
//!   `server.handle(&Request::new("GET", "/api/state"))`.
//! * [`Router`] matches `"METHOD /literal/{param}/segments"` patterns (and the catch-all `"GET /"`). A path that
//!   exists for another method answers 405, an unknown path answers 404 `404 page not found\n`.
//! * [`HttpServer`] runs a handler on `tiny_http`, one thread per request.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;

use kiki_util::Ctx;

use crate::api_error::{respond as respond_api, ApiError, ApiResult};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;

/// Largest request body `read_json` accepts .
pub const MAX_JSON_BODY: usize = 1 << 20;

/// An incoming request.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    /// URL path without the query string, e.g. `/api/tasks/ab12`.
    pub path: String,
    /// Decoded query parameters; repeated keys keep every value.
    pub query: BTreeMap<String, Vec<String>>,
    /// Header names are lowercased.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    /// Path parameters filled in by the router.
    pub params: BTreeMap<String, String>,
    /// Request context; never finishes unless a caller cancels it.
    pub ctx: Ctx,
}

impl Request {
    /// `target` is a path with an optional `?query`, e.g. `"/api/team/diff?member=a&kind=agents"`.
    pub fn new(method: &str, target: &str) -> Request {
        let (path, raw_query) = match target.split_once('?') {
            Some((p, q)) => (p, q),
            None => (target, ""),
        };
        let mut query: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            query.entry(k.into_owned()).or_default().push(v.into_owned());
        }
        Request {
            method: method.to_ascii_uppercase(),
            path: path.to_string(),
            query,
            headers: BTreeMap::new(),
            body: Vec::new(),
            params: BTreeMap::new(),
            ctx: Ctx::background(),
        }
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Request {
        self.body = body.into();
        self
    }

    /// Sets a JSON body and `Content-Type`.
    pub fn with_json<T: Serialize>(mut self, value: &T) -> Request {
        self.body = serde_json::to_vec(value).unwrap_or_default();
        self.headers.insert("content-type".into(), "application/json".into());
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Request {
        self.headers.insert(name.to_ascii_lowercase(), value.to_string());
        self
    }

    pub fn with_ctx(mut self, ctx: Ctx) -> Request {
        self.ctx = ctx;
        self
    }

    /// The value of path parameter `name`; empty when absent.
    pub fn path_value(&self, name: &str) -> &str {
        self.params.get(name).map(String::as_str).unwrap_or("")
    }

    /// The first value of query parameter `name`, or empty.
    pub fn query_get(&self, name: &str) -> &str {
        self.query
            .get(name)
            .and_then(|v| v.first())
            .map(String::as_str)
            .unwrap_or("")
    }

    /// The value of request header `name`; empty when absent.
    pub fn header(&self, name: &str) -> &str {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// Writes a streamed body into the connection.
pub type StreamWriter = Box<dyn FnOnce(&mut dyn Write) + Send>;

/// Response body: fully buffered, or produced by a writer callback (streaming, e.g. NDJSON).
pub enum Body {
    Bytes(Vec<u8>),
    /// Called once with a sink. Every `flush()` on the sink is pushed to the client.
    Stream(StreamWriter),
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Body::Bytes(b) => write!(f, "Body::Bytes({} bytes)", b.len()),
            Body::Stream(_) => f.write_str("Body::Stream"),
        }
    }
}

/// An outgoing response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    /// Canonical header names, in insertion order.
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    pub fn new(status: u16) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: Body::Bytes(Vec::new()),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Response {
        self.set_header(name, value);
        self
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Response {
        self.body = Body::Bytes(body.into());
        self
    }

    /// Sets a header, replacing an earlier value.
    pub fn set_header(&mut self, name: &str, value: &str) {
        let name = canonical_header(name);
        if let Some(h) = self.headers.iter_mut().find(|(n, _)| *n == name) {
            h.1 = value.to_string();
        } else {
            self.headers.push((name, value.to_string()));
        }
    }

    /// The value of response header `name`; empty when absent.
    pub fn header(&self, name: &str) -> &str {
        let name = canonical_header(name);
        self.headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }

    /// Streaming response; `f` receives the sink once the headers have been sent.
    pub fn stream(status: u16, content_type: &str, f: impl FnOnce(&mut dyn Write) + Send + 'static) -> Response {
        let mut r = Response::new(status).with_header("Content-Type", content_type);
        r.body = Body::Stream(Box::new(f));
        r
    }

    /// Collects the body (running a streaming callback into memory). For tests.
    pub fn into_bytes(self) -> Vec<u8> {
        match self.body {
            Body::Bytes(b) => b,
            Body::Stream(f) => {
                let mut buf: Vec<u8> = Vec::new();
                f(&mut buf);
                buf
            }
        }
    }

    /// Body as text (see [`Response::into_bytes`]).
    pub fn into_text(self) -> String {
        String::from_utf8_lossy(&self.into_bytes()).into_owned()
    }

    /// Body parsed as JSON.
    pub fn json<T: DeserializeOwned>(self) -> serde_json::Result<T> {
        serde_json::from_slice(&self.into_bytes())
    }
}

fn canonical_header(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// A JSON reply: `application/json; charset=utf-8`, the value, then a newline. `<`, `>`, `&` are escaped as
/// `\u003c`, `\u003e`, `\u0026` and U+2028/U+2029 as `\u2028`/`\u2029` (the form the web UI always received).
pub fn write_json<T: Serialize>(status: u16, value: &T) -> Response {
    let mut body = json_bytes(value);
    body.push(b'\n');
    Response::new(status)
        .with_header("Content-Type", "application/json; charset=utf-8")
        .with_body(body)
}

/// An error reply: `{"error": "<message>"}`.
pub fn write_err(status: u16, err: &dyn std::fmt::Display) -> Response {
    let mut m = BTreeMap::new();
    m.insert("error", err.to_string());
    write_json(status, &m)
}

/// [`write_err`] for an `anyhow` error; the whole cause chain is printed as `outer: inner`.
pub fn write_anyhow(status: u16, err: &anyhow::Error) -> Response {
    write_err(status, &format!("{err:#}"))
}

/// A plain-text error reply: body `msg\n`.
pub fn plain_error(status: u16, msg: &str) -> Response {
    Response::new(status)
        .with_header("Content-Type", "text/plain; charset=utf-8")
        .with_header("X-Content-Type-Options", "nosniff")
        .with_body(format!("{msg}\n"))
}

/// Compact JSON bytes, HTML-safe (see [`write_json`]).
pub fn json_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let raw = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec());
    let text = String::from_utf8_lossy(&raw);
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out.into_bytes()
}

/// Decodes the body (max 1 MiB). On failure the error is the 400 `{"error":"invalid data"}` reply, which converts
/// into an [`ApiError`](crate::api_error::ApiError), so handlers write `let body: T = read_json(req)?;`.
#[allow(clippy::result_large_err)]
pub fn read_json<T: DeserializeOwned>(req: &Request) -> Result<T, Response> {
    if req.body.len() > MAX_JSON_BODY {
        return Err(write_err(400, &"invalid data"));
    }
    serde_json::from_slice::<T>(&req.body).map_err(|_| write_err(400, &"invalid data"))
}

/// Decodes a JSON `null` as the type's default (a missing list and `null` mean the same thing).
pub fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Decodes an optional body: an empty body (or `null`) is the default value, anything else must be valid JSON.
/// A failure is the 400 `invalid data` error.
pub fn read_optional_json<T: DeserializeOwned + Default>(req: &Request) -> ApiResult<T> {
    if req.body.iter().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')) {
        return Ok(T::default());
    }
    serde_json::from_slice::<Option<T>>(&req.body)
        .map(Option::unwrap_or_default)
        .map_err(|_| ApiError::bad_request("invalid data"))
}

/// A route handler.
pub type Handler<S> = Box<dyn Fn(&S, &Request) -> Response + Send + Sync>;

struct Route<S> {
    method: String,
    segments: Vec<Segment>,
    /// `"GET /"` style catch-all.
    catch_all: bool,
    handler: Handler<S>,
}

enum Segment {
    Literal(String),
    Param(String),
}

/// Routes requests to handlers over shared state `S`.
pub struct Router<S> {
    routes: Vec<Route<S>>,
}

impl<S> Default for Router<S> {
    fn default() -> Self {
        Router { routes: Vec::new() }
    }
}

impl<S> Router<S> {
    pub fn new() -> Router<S> {
        Router::default()
    }

    /// Register `"METHOD /path/{param}"`.
    pub fn handle<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(&S, &Request) -> Response + Send + Sync + 'static,
    {
        let (method, path) = pattern.split_once(' ').expect("pattern needs a method");
        let catch_all = path == "/";
        let segments = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with('{') && s.ends_with('}') {
                    Segment::Param(s[1..s.len() - 1].to_string())
                } else {
                    Segment::Literal(s.to_string())
                }
            })
            .collect();
        self.routes.push(Route {
            method: method.to_string(),
            segments,
            catch_all,
            handler: Box::new(handler),
        });
    }

    /// Registers a handler that returns an [`ApiResult`]: a failure becomes the JSON error reply.
    pub fn api<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(&S, &Request) -> ApiResult<Response> + Send + Sync + 'static,
    {
        self.handle(pattern, move |state, req| respond_api(handler(state, req)));
    }

    /// Runs the matching handler (specific patterns beat the `/` catch-all).
    pub fn dispatch(&self, state: &S, req: &Request) -> Response {
        let parts: Vec<&str> = req
            .path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        let mut path_exists = false;
        let mut best: Option<(&Route<S>, BTreeMap<String, String>)> = None;
        for route in &self.routes {
            let params = if route.catch_all {
                Some(BTreeMap::new())
            } else {
                match_segments(&route.segments, &parts, req.path.ends_with('/') && req.path != "/")
            };
            let Some(params) = params else { continue };
            path_exists = true;
            let method_ok = route.method == req.method || (route.method == "GET" && req.method == "HEAD");
            if !method_ok {
                continue;
            }
            // Prefer specific routes over the catch-all.
            match &best {
                Some((b, _)) if !b.catch_all => {}
                _ if route.catch_all && best.is_some() => {}
                _ => best = Some((route, params)),
            }
        }
        match best {
            Some((route, params)) => {
                let mut req = req.clone();
                req.params = params;
                (route.handler)(state, &req)
            }
            None if path_exists => {
                let mut allowed: Vec<&str> = self
                    .routes
                    .iter()
                    .filter(|r| r.catch_all || match_segments(&r.segments, &parts, false).is_some())
                    .map(|r| r.method.as_str())
                    .collect();
                if allowed.contains(&"GET") {
                    allowed.push("HEAD");
                }
                allowed.sort();
                allowed.dedup();
                plain_error(405, "Method Not Allowed").with_header("Allow", &allowed.join(", "))
            }
            None => plain_error(404, "404 page not found"),
        }
    }
}

fn match_segments(pattern: &[Segment], parts: &[&str], trailing_slash: bool) -> Option<BTreeMap<String, String>> {
    if trailing_slash || pattern.len() != parts.len() {
        return None;
    }
    let mut params = BTreeMap::new();
    for (seg, part) in pattern.iter().zip(parts) {
        match seg {
            Segment::Literal(l) if l == part => {}
            Segment::Literal(_) => return None,
            Segment::Param(name) => {
                let decoded = url::form_urlencoded::parse(format!("v={}", part.replace('+', "%2B")).as_bytes())
                    .next()
                    .map(|(_, v)| v.into_owned())
                    .unwrap_or_else(|| part.to_string());
                params.insert(name.clone(), decoded);
            }
        }
    }
    Some(params)
}

/// A running HTTP listener.
pub struct HttpServer {
    server: Arc<tiny_http::Server>,
    local_addr: String,
}

impl HttpServer {
    /// Bind `addr` (e.g. `127.0.0.1:8787`). Nothing is served until [`HttpServer::serve`].
    pub fn bind(addr: &str) -> anyhow::Result<HttpServer> {
        let server = tiny_http::Server::http(addr).map_err(|e| anyhow::anyhow!("listen {addr}: {e}"))?;
        let local_addr = server.server_addr().to_string();
        Ok(HttpServer {
            server: Arc::new(server),
            local_addr,
        })
    }

    /// The bound address (useful with port 0).
    pub fn local_addr(&self) -> &str {
        &self.local_addr
    }

    /// Accept requests until [`HttpServer::shutdown`]; blocks the calling thread.
    pub fn serve<F>(&self, handler: F)
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        for mut raw in self.server.incoming_requests() {
            let handler = handler.clone();
            std::thread::spawn(move || {
                let method = raw.method().as_str().to_string();
                let target = raw.url().to_string();
                let mut req = Request::new(&method, &target);
                for h in raw.headers() {
                    req.headers.insert(
                        h.field.as_str().as_str().to_ascii_lowercase(),
                        h.value.as_str().to_string(),
                    );
                }
                // Read one byte past the limit so read_json can reject oversized bodies.
                let mut body = Vec::new();
                let _ = raw.as_reader().take(MAX_JSON_BODY as u64 + 1).read_to_end(&mut body);
                req.body = body;
                let is_head = method == "HEAD";
                let resp = handler(req);
                respond(raw, resp, is_head);
            });
        }
    }

    /// Stop accepting requests; `serve` returns.
    pub fn shutdown(&self) {
        self.server.unblock();
    }

    /// A handle that can stop the server from another thread.
    pub fn stopper(&self) -> impl Fn() + Send + Sync + 'static {
        let server = self.server.clone();
        move || server.unblock()
    }
}

fn respond(raw: tiny_http::Request, resp: Response, is_head: bool) {
    let Response { status, headers, body } = resp;
    let mut tiny_headers: Vec<tiny_http::Header> = headers
        .iter()
        .filter_map(|(n, v)| tiny_http::Header::from_bytes(n.as_bytes(), v.as_bytes()).ok())
        .collect();
    match body {
        Body::Bytes(bytes) => {
            let len = bytes.len();
            let reader: Box<dyn Read + Send> = if is_head {
                Box::new(std::io::empty())
            } else {
                Box::new(std::io::Cursor::new(bytes))
            };
            let r = tiny_http::Response::new(tiny_http::StatusCode(status), tiny_headers, reader, Some(len), None);
            let _ = raw.respond(r);
        }
        Body::Stream(f) => {
            // Chunked streaming: run the callback on a worker thread feeding a pipe that tiny_http reads.
            let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
            std::thread::spawn(move || {
                let mut sink = ChannelSink { tx, buf: Vec::new() };
                f(&mut sink);
                let _ = sink.flush();
            });
            tiny_headers.push(tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap());
            let reader = ChannelReader {
                rx,
                cur: std::io::Cursor::new(Vec::new()),
            };
            let r = tiny_http::Response::new(tiny_http::StatusCode(status), tiny_headers, reader, None, None);
            let _ = raw.respond(r);
        }
    }
}

struct ChannelSink {
    tx: std::sync::mpsc::Sender<Vec<u8>>,
    buf: Vec<u8>,
}

impl Write for ChannelSink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let chunk = std::mem::take(&mut self.buf);
        self.tx
            .send(chunk)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "client went away"))
    }
}

struct ChannelReader {
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    cur: std::io::Cursor<Vec<u8>>,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let n = self.cur.read(out)?;
            if n > 0 {
                return Ok(n);
            }
            match self.rx.recv() {
                Ok(chunk) => self.cur = std::io::Cursor::new(chunk),
                Err(_) => return Ok(0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct State;

    fn router() -> Router<State> {
        let mut r = Router::new();
        r.handle("GET /api/tasks/{id}", |_, req| write_json(200, &req.path_value("id")));
        r.handle("PUT /api/tasks/{id}", |_, _| write_json(200, &"put"));
        r.handle("GET /app.css", |_, _| Response::new(200).with_body("css"));
        r.handle("GET /", |_, _| Response::new(200).with_body("page"));
        r.api("GET /api/fails", |_, _| Err(ApiError::bad_request("nope")));
        r
    }

    #[test]
    fn routes_specific_before_catch_all() {
        let r = router();
        assert_eq!(r.dispatch(&State, &Request::new("GET", "/app.css")).into_text(), "css");
        assert_eq!(r.dispatch(&State, &Request::new("GET", "/")).into_text(), "page");
        assert_eq!(
            r.dispatch(&State, &Request::new("GET", "/anything/else")).into_text(),
            "page"
        );
    }

    #[test]
    fn path_params_and_method_mismatch() {
        let r = router();
        let resp = r.dispatch(&State, &Request::new("GET", "/api/tasks/ab12"));
        assert_eq!(resp.status, 200);
        assert_eq!(resp.into_text(), "\"ab12\"\n");
        let resp = r.dispatch(&State, &Request::new("DELETE", "/api/tasks/ab12"));
        assert_eq!(resp.status, 405);
        assert_eq!(resp.header("Allow"), "GET, HEAD, PUT");
    }

    #[test]
    fn api_handlers_turn_errors_into_json_replies() {
        let reply = router().dispatch(&State, &Request::new("GET", "/api/fails"));
        assert_eq!(reply.status, 400);
        assert_eq!(reply.into_text(), "{\"error\":\"nope\"}\n");
    }

    #[test]
    fn json_is_html_safe() {
        let resp = write_json(200, &"<a&b>");
        assert_eq!(resp.into_text(), "\"\\u003ca\\u0026b\\u003e\"\n");
    }

    #[test]
    fn read_json_rejects_garbage() {
        let req = Request::new("POST", "/x").with_body("not json");
        let err = read_json::<serde_json::Value>(&req).unwrap_err();
        assert_eq!(err.status, 400);
        assert_eq!(err.into_text(), "{\"error\":\"invalid data\"}\n");
    }

    #[test]
    fn query_parsing() {
        let req = Request::new("GET", "/api/team/diff?member=a%20b&kind=agents");
        assert_eq!(req.path, "/api/team/diff");
        assert_eq!(req.query_get("member"), "a b");
        assert_eq!(req.query_get("kind"), "agents");
    }
}
