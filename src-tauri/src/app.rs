//! Starts KIKI: the workspace, the loopback listener, and (unless `--no-open`) the Tauri window.

use anyhow::Result;
use tauri::{Manager, RunEvent};

use crate::bootstrap::Workspace;
use crate::cli::Options;
use crate::http_adapter::Listener;
use crate::ipc::{self, AppState};
use crate::menu;

pub fn launch(options: Options) -> Result<()> {
    let workspace = Workspace::open(options.root.as_deref())?;
    let url = format!("http://{}/", options.addr);
    let listener = Listener::spawn(workspace.server.clone(), &options.addr)?;

    if let Err(err) = workspace.server.enable_guardrails(&options.addr) {
        println!("Guardrails: {err:#}");
    }
    if let Err(err) = workspace.server.sync_rtk() {
        println!("RTK: {err:#}");
    }
    println!("{}", workspace.summary(&url));

    if options.no_open {
        listener.wait();
        return Ok(());
    }

    let state = AppState {
        server: workspace.server.clone(),
    };
    let app = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![ipc::api_request])
        .setup(|app| {
            menu::install(app)?;
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                crate::window::allow_fullscreen_space(&window);
            }
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(move |_handle, event| {
        if let RunEvent::Exit = event {
            let _ = workspace.store.close();
        }
    });
    listener.shutdown();
    Ok(())
}
