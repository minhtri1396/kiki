//! Loopback HTTP adapter over the same core as the IPC adapter.
//!
//! The UI files are served by [`crate::assets`]; everything else goes to the core.
//!
//! It exists because the guardrail hooks run in separate processes (the AI tools start them) and reach KIKI
//! only through `POST /api/guardrail`. With `--no-open` it also serves the web UI to a browser.

use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::Result;
use kiki_server::http::HttpServer;
use kiki_server::Server;

/// A running listener.
pub struct Listener {
    stop: Box<dyn Fn() + Send + Sync>,
    thread: JoinHandle<()>,
}

impl Listener {
    /// Binds `addr` and serves every request through `server.handle` on background threads.
    pub fn spawn(server: Arc<Server>, addr: &str) -> Result<Listener> {
        let http = HttpServer::bind(addr)?;
        let stop = Box::new(http.stopper());
        let thread = std::thread::spawn(move || {
            http.serve(move |request| crate::assets::serve(&request).unwrap_or_else(|| server.handle(&request)))
        });
        Ok(Listener { stop, thread })
    }

    /// Stops accepting requests.
    pub fn shutdown(self) {
        (self.stop)();
        let _ = self.thread.join();
    }

    /// Blocks until the listener stops (headless mode).
    pub fn wait(self) {
        let _ = self.thread.join();
    }
}
