//! Tauri IPC adapter: the web UI calls `api_request(method, path, body, onEvent)` instead of `fetch`.
//!
//! The command replays what an HTTP response is: a `head` (status and headers), zero or more `chunk`s
//! (base64 bytes, flushed as the handler produces them, which is how NDJSON generation streams), and `end`.
//! `src/app.js` turns that back into a `Response` so the rest of the UI is unchanged.

use std::io::Write;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use kiki_server::http::{Body, Request};
use kiki_server::Server;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

/// Shared state of the app window: the transport-agnostic core.
pub struct AppState {
    pub server: Arc<Server>,
}

/// One message of the reply to `api_request`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum ApiEvent {
    Head {
        status: u16,
        headers: Vec<(String, String)>,
    },
    /// Base64 of the next bytes of the body.
    Chunk(String),
    End,
}

#[tauri::command]
pub async fn api_request(
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<String>,
    on_event: Channel<ApiEvent>,
) -> Result<(), String> {
    let server = state.server.clone();
    // Handlers block on files, processes, and the network: keep them off the async runtime threads.
    tauri::async_runtime::spawn_blocking(move || {
        dispatch(&server, &method, &path, body, |event| {
            let _ = on_event.send(event);
        })
    })
    .await
    .map_err(|err| err.to_string())
}

/// Runs one request through the core and reports it as events. Independent of Tauri so it can be tested.
pub fn dispatch(server: &Server, method: &str, target: &str, body: Option<String>, mut emit: impl FnMut(ApiEvent)) {
    let mut request = Request::new(method, target);
    if let Some(body) = body.filter(|b| !b.is_empty()) {
        request = request.with_body(body).with_header("content-type", "application/json");
    }
    let response = server.handle(&request);
    emit(ApiEvent::Head {
        status: response.status,
        headers: response.headers,
    });
    match response.body {
        Body::Bytes(bytes) => {
            if !bytes.is_empty() {
                emit(ApiEvent::Chunk(STANDARD.encode(bytes)));
            }
        }
        Body::Stream(produce) => {
            let mut sink = EventSink { emit: &mut emit };
            produce(&mut sink);
            let _ = sink.flush();
        }
    }
    emit(ApiEvent::End);
}

/// Forwards each flushed write of a streaming handler as one `chunk`.
struct EventSink<'a, F: FnMut(ApiEvent)> {
    emit: &'a mut F,
}

impl<F: FnMut(ApiEvent)> Write for EventSink<'_, F> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if !data.is_empty() {
            (self.emit)(ApiEvent::Chunk(STANDARD.encode(data)));
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::Workspace;

    fn collect(server: &Server, method: &str, path: &str, body: Option<&str>) -> Vec<ApiEvent> {
        let mut events = Vec::new();
        dispatch(server, method, path, body.map(str::to_string), |event| {
            events.push(event)
        });
        events
    }

    fn body_text(events: &[ApiEvent]) -> String {
        let bytes: Vec<u8> = events
            .iter()
            .filter_map(|event| match event {
                ApiEvent::Chunk(data) => Some(STANDARD.decode(data).unwrap()),
                _ => None,
            })
            .flatten()
            .collect();
        String::from_utf8(bytes).unwrap()
    }

    fn workspace() -> (Workspace, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(dir.path().to_str()).unwrap();
        (workspace, dir)
    }

    #[test]
    fn a_reply_is_head_then_body_then_end() {
        let (workspace, _dir) = workspace();
        let events = collect(&workspace.server, "GET", "/api/state", None);
        assert!(
            matches!(events.first(), Some(ApiEvent::Head { status: 200, .. })),
            "events = {events:?}"
        );
        assert_eq!(events.last(), Some(&ApiEvent::End));
        let state: serde_json::Value = serde_json::from_str(&body_text(&events)).unwrap();
        assert!(state.get("agents").is_some());
    }

    #[test]
    fn errors_keep_their_status_and_json_body() {
        let (workspace, _dir) = workspace();
        let events = collect(&workspace.server, "POST", "/api/groups", Some(r#"{"name":" "}"#));
        assert!(
            matches!(events.first(), Some(ApiEvent::Head { status: 400, .. })),
            "events = {events:?}"
        );
        assert!(body_text(&events).contains("Enter a group name."));
    }

    #[test]
    fn an_unknown_route_is_a_404_and_an_empty_body_sends_no_chunk() {
        let (workspace, _dir) = workspace();
        let events = collect(&workspace.server, "DELETE", "/api/launches/none", None);
        assert!(matches!(events.first(), Some(ApiEvent::Head { .. })));
        let missing = collect(&workspace.server, "GET", "/api/missing", None);
        assert!(matches!(missing.first(), Some(ApiEvent::Head { status: 404, .. })));
        assert_eq!(missing.last(), Some(&ApiEvent::End));
    }

    #[test]
    fn each_write_of_a_stream_becomes_its_own_chunk() {
        let mut chunks = Vec::new();
        let mut sink = EventSink {
            emit: &mut |event| chunks.push(event),
        };
        sink.write_all(b"{\"type\":\"meta\"}\n").unwrap();
        sink.write_all(b"{\"type\":\"done\"}\n").unwrap();
        assert_eq!(
            chunks,
            vec![
                ApiEvent::Chunk(STANDARD.encode(b"{\"type\":\"meta\"}\n")),
                ApiEvent::Chunk(STANDARD.encode(b"{\"type\":\"done\"}\n")),
            ]
        );
    }

    #[test]
    fn events_serialize_the_way_the_web_ui_reads_them() {
        let head = serde_json::to_value(ApiEvent::Head {
            status: 200,
            headers: vec![("a".into(), "b".into())],
        })
        .unwrap();
        assert_eq!(
            head,
            serde_json::json!({"event": "head", "data": {"status": 200, "headers": [["a", "b"]]}})
        );
        assert_eq!(
            serde_json::to_value(ApiEvent::Chunk("eA==".into())).unwrap(),
            serde_json::json!({"event": "chunk", "data": "eA=="})
        );
        assert_eq!(
            serde_json::to_value(ApiEvent::End).unwrap(),
            serde_json::json!({"event": "end"})
        );
    }
}
