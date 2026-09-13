//! macOS window behaviour (§6.2).

use objc2::rc::Retained;
use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
use tauri::WebviewWindow;

/// Set the collection behaviour Tauri does not expose.
///
/// `always_on_top` already gives the floating window level, and
/// `visible_on_all_workspaces` gives `CanJoinAllSpaces`. Neither sets
/// `FullScreenAuxiliary` — and without it Clawd vanishes the moment the user
/// fullscreens their terminal or editor, which is precisely when they are
/// least able to see what Claude is doing.
///
/// `Stationary` is added for the same reason: it keeps the pet from being
/// swept along by Mission Control's Space transitions.
pub fn configure(window: &WebviewWindow) {
    let Ok(handle) = window.ns_window() else {
        return;
    };
    if handle.is_null() {
        return;
    }

    // SAFETY: `ns_window` hands back the window's own NSWindow pointer, which
    // is live for as long as the window is, and this runs on the main thread
    // during setup.
    unsafe {
        let ns_window: Retained<NSWindow> =
            Retained::retain(handle.cast::<NSWindow>()).expect("live NSWindow");
        let behaviour = ns_window.collectionBehavior()
            | NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary;
        ns_window.setCollectionBehavior(behaviour);
    }
}
