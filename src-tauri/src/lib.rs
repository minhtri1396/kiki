//! KIKI desktop shell.
//!
//! The Rust core (`crates/*`) is transport-agnostic: `kiki_server::Server::handle` turns a request into a
//! response. This shell adapts it to two inbound transports:
//!
//! * **Tauri IPC** ([`ipc`]): the web UI in the app window calls the `api_request` command.
//! * **Loopback HTTP** ([`http_adapter`]): the guardrail hooks that AI tools run as separate processes
//!   post to `/api/guardrail`, and `--no-open` serves the same UI to a browser.
//!
//! Pattern: Adapter (both transports) over a Facade (`Server`); `Command` for the parsed command line.

mod app;
mod assets;
mod bootstrap;
mod cli;
mod http_adapter;
mod ipc;
mod menu;
#[cfg(target_os = "macos")]
mod window;

/// Entry point shared by the desktop binary (and a mobile one, should it ever exist).
pub fn run() {
    let code = match cli::Cli::parse(std::env::args().skip(1)) {
        Ok(cli::Cli::Guardrail { tool }) => kiki_guard::run(&tool),
        Ok(cli::Cli::Serve(options)) => match app::launch(options) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("{err:#}");
                1
            }
        },
        Err(err) => {
            eprintln!("{err}");
            2
        }
    };
    if code != 0 {
        std::process::exit(code);
    }
}
