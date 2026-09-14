//! Cursor-poll hit testing (§6.1).
//!
//! The pet occupies a known box at a known offset inside the window, so this
//! is a rectangle test rather than a per-pixel mask — which means one shared
//! Rust implementation and no per-platform native code. The window is
//! therefore free to be larger than the sprite, letting the pulse ring, the
//! badge and the label chip overflow harmlessly.
//!
//! The cost is one always-running low-frequency timer. Cheap, but it counts
//! against the idle budget in §9.

use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::app_state::Clawd;

/// Low enough to feel instant, high enough to stay off the idle budget.
const POLL: Duration = Duration::from_millis(80);
/// A few pixels of slack so the boundary does not chatter as it is crossed.
const HYSTERESIS: f64 = 4.0;

pub fn spawn(app: AppHandle, window: WebviewWindow) {
    thread::spawn(move || loop {
        thread::sleep(POLL);
        if !step(&app, &window) {
            break;
        }
    });
}

/// One poll. Returns false when the window is gone and the loop should stop.
fn step(app: &AppHandle, window: &WebviewWindow) -> bool {
    let Some(state) = app.try_state::<Clawd>() else {
        return false;
    };

    // A drag already owns the pointer, and a hidden window has nothing to hit.
    if state.is_dragging() {
        return true;
    }
    match window.is_visible() {
        Ok(false) => return true,
        Err(_) => return false,
        Ok(true) => {}
    }

    let (Ok(cursor), Ok(origin)) = (window.cursor_position(), window.outer_position()) else {
        return true;
    };

    let layout = *state.layout.lock().unwrap();
    let was_interactive = state.is_interactive();
    // Slack only applies on the way out, so the pet is easy to leave but
    // lands exactly on its edge on the way in.
    let margin = if was_interactive { HYSTERESIS } else { 0.0 };
    let inside = layout
        .hit_rect(origin.x as f64, origin.y as f64)
        .contains(cursor.x, cursor.y, margin);

    if inside != was_interactive {
        state.set_interactive(inside);
        let _ = window.set_ignore_cursor_events(!inside);
    }
    true
}
