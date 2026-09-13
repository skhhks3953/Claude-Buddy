//! Per-platform window configuration.
//!
//! Almost everything Clawd needs is available through Tauri's own API. These
//! two are not, and both are the difference between the app working and the
//! app quietly being useless.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use tauri::WebviewWindow;

pub fn configure(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::configure(window);
    #[cfg(target_os = "windows")]
    windows::configure(window);
    // Linux is out of scope for v1 (§2.1): Wayland has no global coordinate
    // system, so set_position silently does nothing and always_on_top is
    // unsupported. That removes staying above other windows, opening in a
    // default corner, and drag-to-reposition — the three things this app is.
    // X11 works, and Tauri under XWayland inherits X11 behaviour, so this is
    // deferred rather than impossible.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = window;
}
