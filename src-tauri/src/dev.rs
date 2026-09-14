//! The developer harness (§8).
//!
//! Compiled only under the `devtools` feature, so no path exists by which a
//! debug surface reaches a user (§8.3). Run it with `npm run tauri:dev`.
//!
//! Everything here injects into `MockSource` and lets the event flow through
//! the real state machine, the real IPC and the real renderer. Nothing writes
//! to the view. That distinction is the whole point: a dev panel that wrote
//! straight to the view would let the state machine be wrong while every state
//! still looked correct.

use std::time::Instant;

use clawd_core::event::EventKind;
use clawd_core::mock::{self, PRIMARY_SESSION, SECOND_SESSION};
use clawd_core::{ClawdState, SessionEvent, Timings};
use tauri::menu::{IsMenuItem, MenuItem};
use tauri::{AppHandle, Manager, State, Wry};

use crate::app_state::Clawd;

const SCRIPT: &str = "script";
const SECOND: &str = "second-session";
const FAST_TIMERS: &str = "fast-timers";

/// Injected by the number keys and the dev tray items.
#[tauri::command]
pub fn dev_inject(state: State<Clawd>, action: String) {
    run(&state, &action);
}

fn run(state: &Clawd, action: &str) {
    match action {
        // A realistic timed sequence, which is what surfaces ugly transitions
        // and label thrash (§8.2).
        SCRIPT => state.mock.play_script(),

        // Prove the stickiness of blocking states against a chatty neighbour.
        SECOND => state.mock.inject(SessionEvent::new(
            SECOND_SESSION,
            EventKind::ToolStarted {
                name: "Bash".into(),
                target: Some("cargo build".into()),
            },
        )),

        // Shrink the 10s long-task threshold and the watchdog so both are
        // testable without waiting around.
        FAST_TIMERS => {
            state
                .machine
                .lock()
                .unwrap()
                .set_timings(Timings::fast(), Instant::now());
            log("timers shrunk: long task 2s, success 2s, watchdog 6s");
        }

        // A state name from the number keys. Note this asks for the *event*
        // that earns the state — pressing 9 starts a tool call, and the
        // long-task pose arrives only when the timer says so.
        name => match ClawdState::parse(name) {
            Some(state_name) => state
                .mock
                .inject(mock::event_for_state(state_name, PRIMARY_SESSION)),
            None => log(&format!("unknown dev action: {name}")),
        },
    }
}

fn log(message: &str) {
    eprintln!("[clawd dev] {message}");
}

/// The dev entries added to the tray menu.
pub fn menu_items(app: &AppHandle) -> tauri::Result<Vec<Box<dyn IsMenuItem<Wry>>>> {
    Ok(vec![
        Box::new(MenuItem::with_id(
            app,
            SCRIPT,
            "Dev · scripted run",
            true,
            None::<&str>,
        )?),
        Box::new(MenuItem::with_id(
            app,
            SECOND,
            "Dev · inject second session",
            true,
            None::<&str>,
        )?),
        Box::new(MenuItem::with_id(
            app,
            FAST_TIMERS,
            "Dev · shrink timers",
            true,
            None::<&str>,
        )?),
    ])
}

pub fn handle_menu(app: &AppHandle, id: &str) {
    if let Some(state) = app.try_state::<Clawd>() {
        run(&state, id);
    }
}
