//! Pushing state down to the webview, and taking hover and drag back up
//! (§3.1 `ipc`).

use clawd_core::layout::Layout;
use clawd_core::machine::Snapshot;
use serde::Serialize;
use tauri::{AppHandle, Emitter, PhysicalPosition, State, WebviewWindow};

use crate::app_state::Clawd;
use crate::position;
use crate::window;

pub const SNAPSHOT_EVENT: &str = "clawd://snapshot";
pub const LAYOUT_EVENT: &str = "clawd://layout";
pub const OCCLUSION_EVENT: &str = "clawd://occlusion";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LayoutPayload {
    sprite_size: f64,
}

pub fn push_snapshot(app: &AppHandle, snapshot: &Snapshot) {
    let _ = app.emit(SNAPSHOT_EVENT, snapshot);
}

pub fn push_layout(app: &AppHandle, layout: &Layout) {
    let _ = app.emit(
        LAYOUT_EVENT,
        LayoutPayload {
            sprite_size: layout.sprite_css(),
        },
    );
}

pub fn push_occlusion(app: &AppHandle, occluded: bool) {
    let _ = app.emit(OCCLUSION_EVENT, occluded);
}

/// The webview asking for the current state, on mount.
///
/// The window can be reloaded mid-session, and waiting for the next state
/// change would leave the pet showing `Idle` while a tool was running.
#[tauri::command]
pub fn hello(app: AppHandle, state: State<Clawd>) {
    let snapshot = state.machine.lock().unwrap().snapshot();
    let layout = *state.layout.lock().unwrap();
    push_snapshot(&app, &snapshot);
    push_layout(&app, &layout);
}

/// One step of a manual drag (§6.5).
///
/// The deltas arrive in logical pixels from the webview's pointer capture.
#[tauri::command]
pub fn move_window(window: WebviewWindow, state: State<Clawd>, dx: f64, dy: f64) {
    state.set_dragging(true);

    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(current) = window.outer_position() else {
        return;
    };

    // Carry the sub-pixel remainder rather than rounding each delta on its
    // own, which would let the window drift behind the cursor on a fractional
    // scale factor.
    let (step_x, step_y) = {
        let mut residual = state.drag_residual.lock().unwrap();
        let fx = dx * scale + residual.0;
        let fy = dy * scale + residual.1;
        let ix = fx.trunc();
        let iy = fy.trunc();
        *residual = (fx - ix, fy - iy);
        (ix as i32, iy as i32)
    };

    let _ = window.set_position(PhysicalPosition::new(current.x + step_x, current.y + step_y));
}

/// The drag ended: clamp back into visible bounds, persist per display, and
/// re-run the DPI snap in case the pet crossed onto a different monitor.
#[tauri::command]
pub fn drag_finished(app: AppHandle, window: WebviewWindow, state: State<Clawd>) {
    state.set_dragging(false);
    *state.drag_residual.lock().unwrap() = (0.0, 0.0);

    if let (Some(monitor), Ok(position)) = (position::monitor_for(&window), window.outer_position())
    {
        let layout = *state.layout.lock().unwrap();
        let clamped = position::clamp_to(&monitor, &layout, position.x, position.y);
        if clamped != position {
            let _ = window.set_position(clamped);
        }
    }

    window::apply_layout(&app, &window);
    window::remember_position(&app, &window);
}
