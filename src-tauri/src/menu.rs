//! The macOS menu bar. Without it, standard shortcuts (copy, paste, select all) do not reach the web view
//! and the window cannot leave a fullscreen Space.

use tauri::menu::{Menu, SubmenuBuilder};
use tauri::App;

pub fn install(app: &App) -> tauri::Result<()> {
    let app_menu = SubmenuBuilder::new(app, "KIKI").quit_with_text("Quit KIKI").build()?;
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .separator()
        .select_all()
        .build()?;
    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize_with_text("Zoom")
        .separator()
        .fullscreen_with_text("Toggle Full Screen")
        .build()?;
    let menu = Menu::with_items(app, &[&app_menu, &edit_menu, &window_menu])?;
    app.set_menu(menu)?;
    #[cfg(target_os = "macos")]
    window_menu.set_as_windows_menu_for_nsapp()?;
    Ok(())
}
