//! macOS window behaviour that Tauri does not expose.

use objc2::msg_send;
use objc2::runtime::AnyObject;
use tauri::WebviewWindow;

/// A resizable window enters a fullscreen Space from the green button. Without `FullScreenPrimary` and a
/// menu bar, macOS never reveals the title bar there, so the window could not be zoomed back.
pub fn allow_fullscreen_space(window: &WebviewWindow) {
    // NSWindowCollectionBehaviorFullScreenPrimary (1 << 7) | NSWindowCollectionBehaviorManaged (1 << 2)
    const BEHAVIOR: usize = (1 << 7) | (1 << 2);
    let Ok(ns_window) = window.ns_window() else { return };
    let ns_window = ns_window as *mut AnyObject;
    if ns_window.is_null() {
        return;
    }
    // SAFETY: `ns_window` is the live NSWindow of this window, and `setCollectionBehavior:` takes an NSUInteger.
    unsafe {
        let _: () = msg_send![ns_window, setCollectionBehavior: BEHAVIOR];
    }
}
