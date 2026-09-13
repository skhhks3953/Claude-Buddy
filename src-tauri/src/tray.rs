//! The tray icon and its menu (§3.1 `tray`).
//!
//! The prototype's menu implies bidirectional control — "Pause session", "Open
//! terminal" — but hooks are an observation channel. There is no mechanism for
//! an external app to reach into a running session and pause it, resume it, or
//! answer a pending prompt, so those items are not build tasks; they are
//! unavailable (§2.3). What remains is everything Clawd can honestly do.

use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

use crate::position::Corner;
use crate::window;

const TOGGLE: &str = "toggle";
const SNAP_TOP_LEFT: &str = "snap.top-left";
const SNAP_TOP_RIGHT: &str = "snap.top-right";
const SNAP_BOTTOM_LEFT: &str = "snap.bottom-left";
const SNAP_BOTTOM_RIGHT: &str = "snap.bottom-right";
const RESET: &str = "reset";
const QUIT: &str = "quit";

pub fn build(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;

    // Declared before `items`, which borrows them: they must outlive it.
    #[cfg(feature = "devtools")]
    let dev_items = crate::dev::menu_items(app)?;
    #[cfg(feature = "devtools")]
    let dev_separator = PredefinedMenuItem::separator(app)?;

    let toggle = MenuItem::with_id(app, TOGGLE, "Hide Clawd", true, None::<&str>)?;
    let snap = Submenu::with_id_and_items(
        app,
        "snap",
        "Snap to corner",
        true,
        &[
            &MenuItem::with_id(app, SNAP_TOP_LEFT, "Top left", true, None::<&str>)?,
            &MenuItem::with_id(app, SNAP_TOP_RIGHT, "Top right", true, None::<&str>)?,
            &MenuItem::with_id(app, SNAP_BOTTOM_LEFT, "Bottom left", true, None::<&str>)?,
            &MenuItem::with_id(app, SNAP_BOTTOM_RIGHT, "Bottom right", true, None::<&str>)?,
        ],
    )?;
    let reset = MenuItem::with_id(app, RESET, "Reset position", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Clawd", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&toggle, &snap, &reset];

    // Absent from release binaries, not merely hidden (§8.3).
    #[cfg(feature = "devtools")]
    {
        items.push(&dev_separator);
        for item in &dev_items {
            items.push(item.as_ref());
        }
    }

    items.push(&separator);
    items.push(&quit);
    let menu = Menu::with_items(app, &items)?;

    // The toggle's own label has to change as it is used, so the handler keeps
    // a handle to it.
    let toggle_for_handler = toggle.clone();
    TrayIconBuilder::with_id("clawd-tray")
        .icon(icon)
        .tooltip("Clawd")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| handle(app, &event, &toggle_for_handler))
        .build(app)
}

fn handle(app: &AppHandle, event: &MenuEvent, toggle: &MenuItem<Wry>) {
    // Quitting is the one item that does not need the window.
    if event.id() == QUIT {
        app.exit(0);
        return;
    }

    let Some(window) = app.get_webview_window(window::LABEL) else {
        return;
    };

    match event.id().0.as_str() {
        TOGGLE => {
            let visible = window::toggle_visible(app, &window);
            let _ = toggle.set_text(if visible { "Hide Clawd" } else { "Show Clawd" });
        }
        SNAP_TOP_LEFT => window::snap_to(app, &window, Corner::TopLeft),
        SNAP_TOP_RIGHT => window::snap_to(app, &window, Corner::TopRight),
        SNAP_BOTTOM_LEFT => window::snap_to(app, &window, Corner::BottomLeft),
        SNAP_BOTTOM_RIGHT => window::snap_to(app, &window, Corner::BottomRight),
        RESET => window::reset_position(app, &window),
        #[cfg(feature = "devtools")]
        other => crate::dev::handle_menu(app, other),
        #[cfg(not(feature = "devtools"))]
        _ => {}
    }
}
