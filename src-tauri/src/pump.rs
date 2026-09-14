//! The event pump: source → state machine → webview.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::app_state::Clawd;
use crate::ipc;

/// How often the timers are advanced. The machine decides what, if anything,
/// that means — a tick that changes nothing emits nothing, so an idle Clawd
/// costs one wake-up and a comparison.
const TICK: Duration = Duration::from_millis(250);

pub fn spawn(app: AppHandle, events: Receiver<clawd_core::SessionEvent>) {
    thread::spawn(move || loop {
        let event = match events.recv_timeout(TICK) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        let Some(state) = app.try_state::<Clawd>() else {
            break;
        };

        let snapshot = {
            let mut machine = state.machine.lock().unwrap();
            machine.advance(event.as_ref(), Instant::now())
        };

        if let Some(snapshot) = snapshot {
            ipc::push_snapshot(&app, &snapshot);
        }
    });
}
