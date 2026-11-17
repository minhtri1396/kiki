//! Test doubles: a fake [`HttpTransport`], a tiny local HTTP server, and a sample setup.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use kiki_util::Ctx;

use crate::atlassian::{Atlassian, Service};
use crate::http::{HttpRequest, HttpResponse, HttpTransport, Method};
use crate::AtlassianApi;

/// One request a [`FakeTransport`] saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub method: Method,
    pub url: String,
    pub payload: Option<Vec<u8>>,
}

type Handler = dyn Fn(&HttpRequest<'_>) -> Result<HttpResponse> + Send + Sync;

/// Answers every request through `handler` and records what it was asked.
pub struct FakeTransport {
    handler: Box<Handler>,
    pub seen: Mutex<Vec<Seen>>,
}

impl FakeTransport {
    pub fn new<F>(handler: F) -> Arc<FakeTransport>
    where
        F: Fn(&HttpRequest<'_>) -> Result<HttpResponse> + Send + Sync + 'static,
    {
        Arc::new(FakeTransport {
            handler: Box::new(handler),
            seen: Mutex::new(Vec::new()),
        })
    }

    /// An API client that talks to this fake.
    pub fn api(self: &Arc<Self>) -> AtlassianApi {
        AtlassianApi::new(self.clone())
    }
}

impl HttpTransport for FakeTransport {
    fn send(&self, _ctx: &Ctx, request: &HttpRequest<'_>) -> Result<HttpResponse> {
        self.seen.lock().unwrap().push(Seen {
            method: request.method,
            url: request.url.to_string(),
            payload: request.payload.map(<[u8]>::to_vec),
        });
        (self.handler)(request)
    }
}

pub fn ok(status: u16, body: &str) -> Result<HttpResponse> {
    Ok(HttpResponse {
        status,
        body: body.as_bytes().to_vec(),
    })
}

/// (method, url, authorization header, body) of each request the local server saw.
pub type ServerSeen = Arc<Mutex<Vec<(String, String, String, String)>>>;

/// A tiny local server for the real transport. It answers `count` requests with
/// the `(status, body)` returned by `handler(method, url)`.
pub struct Fake {
    pub base: String,
    pub seen: ServerSeen,
}

pub fn serve<F>(count: usize, handler: F) -> Fake
where
    F: Fn(&str, &str) -> (u16, String) + Send + 'static,
{
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let seen: ServerSeen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    std::thread::spawn(move || {
        for _ in 0..count {
            let Ok(mut req) = server.recv() else { return };
            let method = req.method().to_string();
            let url = req.url().to_string();
            let auth = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"))
                .map(|h| h.value.to_string())
                .unwrap_or_default();
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let (status, reply) = handler(&method, &url);
            record.lock().unwrap().push((method, url, auth, body));
            let _ = req.respond(tiny_http::Response::from_string(reply).with_status_code(status));
        }
    });
    Fake { base, seen }
}

pub fn sample_atlassian() -> Atlassian {
    Atlassian {
        enabled: true,
        username: "a@acme.co".into(),
        api_token: "secret-jira".into(),
        jira: Service {
            enabled: true,
            url: "https://acme.atlassian.net".into(),
            ..Default::default()
        },
        confluence: Service {
            enabled: true,
            url: "https://acme.atlassian.net/wiki".into(),
            own: true,
            username: "a@acme.co".into(),
            api_token: "secret-wiki".into(),
            ..Default::default()
        },
    }
}
