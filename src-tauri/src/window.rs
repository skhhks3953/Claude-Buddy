//! The transparent, always-on-top window (§6).

use clawd_core::layout::Layout;
use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::app_state::Clawd;
use crate::ipc;
use crate::platform;
use crate::position::{self, PositionStore};

pub const LABEL: &str = "clawd";

/// Build the pet window.
///
/// Created hidden and positioned before being shown: the DPI snap and the
/// remembered position both depend on which monitor it lands on, and a window
/// that flashes in the wrong corner first is worse than one that appears a
/// frame later.
pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let layout = Layout::default();
    let (w, h) = layout.window_css();

    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::default())
        .title("Clawd")
        .inner_size(w, h)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        // No shadow: Clawd sits on the desktop, it does not float on a card.
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // §6.2 — without this Clawd is stranded on one Space.
        .visible_on_all_workspaces(true)
        // Taking focus would steal the caret from whatever the user is typing
        // in, which for a status pet is unforgivable.
        .focused(false)
        .visible(false)
        .build()?;

    platform::configure(&window);
    Ok(window)
}

/// Place the window and size it to the display it landed on.
pub fn place(app: &AppHandle, window: &WebviewWindow) {
    let monitors = window.available_monitors().unwrap_or_default();
    let primary = window.primary_monitor().ok().flatten();

    let position = {
        let state = app.state::<Clawd>();
        let store = state.positions.lock().unwrap();
        position::startup_placement(&store, &monitors, primary.as_ref()).map(|placement| {
            // The layout comes from the display the pet is actually opening
            // on, not from the primary one — the snapped window size is what
            // the clamp is measured against.
            let layout = Layout::for_scale(placement.monitor.scale_factor());
            placement.resolve(&layout)
        })
    };

    if let Some(position) = position {
        let _ = window.set_position(position);
    }

    // Size the window for the display it landed on.
    apply_layout(app, window);
    let _ = window.show();

    // Start click-through, matching `Clawd`'s initial `interactive: false`.
    // The cursor poll only acts on a *change*, so without this the whole
    // window would swallow clicks until the cursor first entered the sprite
    // and left again.
    //
    // It has to come after `show`: the click-through region is applied to the
    // underlying platform window, which does not exist until the window is
    // realised.
    let _ = window.set_ignore_cursor_events(true);
}

/// Recompute the DPI snap for the window's current display and push it down.
///
/// Called at startup, after a drag, and on a scale-factor change — §6.4
/// requires this to re-run when the window is dragged between displays with
/// different scale factors.
pub fn apply_layout(app: &AppHandle, window: &WebviewWindow) {
    let scale = position::monitor_for(window)
        .map(|m| m.scale_factor())
        .or_else(|| window.scale_factor().ok())
        .unwrap_or(1.0);

    let next = Layout::for_scale(scale);
    *app.state::<Clawd>().layout.lock().unwrap() = next;

    // Applied unconditionally rather than only when the scale changed. This
    // runs at startup, after a drag and on a scale-factor change — rare enough
    // that the saving was never worth leaving the window at whatever size the
    // platform decided to give it.
    let (w, h) = next.window_css();
    let _ = window.set_size(LogicalSize::new(w, h));
    ipc::push_layout(app, &next);
}

/// Persist where the pet was left, for the display it was left on.
pub fn remember_position(app: &AppHandle, window: &WebviewWindow) {
    let (Some(monitor), Ok(position)) = (position::monitor_for(window), window.outer_position())
    else {
        return;
    };
    let state = app.state::<Clawd>();
    let mut store = state.positions.lock().unwrap();
    store.remember(&monitor, position);
    store.save(app);
}

/// Send the window back to its default corner and forget every remembered spot.
pub fn reset_position(app: &AppHandle, window: &WebviewWindow) {
    {
        let state = app.state::<Clawd>();
        let mut store = state.positions.lock().unwrap();
        store.forget_all();
        store.save(app);
    }
    snap_to(app, window, position::Corner::BottomRight);
}

pub fn snap_to(app: &AppHandle, window: &WebviewWindow, corner: position::Corner) {
    let Some(monitor) = position::monitor_for(window) else {
        return;
    };
    let layout = *app.state::<Clawd>().layout.lock().unwrap();
    let _ = window.set_position(position::corner_of(&monitor, &layout, corner));
    remember_position(app, window);
}

/// Show or hide the pet from the tray.
pub fn toggle_visible(app: &AppHandle, window: &WebviewWindow) -> bool {
    let visible = window.is_visible().unwrap_or(true);
    if visible {
        let _ = window.hide();
    } else {
        let _ = window.show();
    }
    // A hidden window is occluded by definition; telling the webview stops it
    // animating into the void (§7.4).
    ipc::push_occlusion(app, visible);
    !visible
}

/// Reload the store from disk. Only used at startup.
pub fn load_positions(app: &AppHandle) -> PositionStore {
    PositionStore::load(app)
}
