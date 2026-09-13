//! Clawd's native shell.
//!
//! Everything that has to survive the webview being occluded, throttled or
//! suspended lives on this side: the state machine, the timers, the window,
//! and the tray. React is a pure render target (§3).

mod app_state;
#[cfg(feature = "devtools")]
mod dev;
mod hit_test;
mod ipc;
mod platform;
mod position;
mod pump;
mod tray;
mod window;

use std::time::Instant;

use clawd_core::machine::Machine;
use clawd_core::mock::{self, MockSource};
use clawd_core::source::{EventSink, EventSource};
use clawd_core::Timings;
use tauri::{Manager, WindowEvent};

use app_state::Clawd;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().setup(|app| {
        let handle = app.handle().clone();

        // The adapter boundary (§3.1). v1 has one implementation; HookSource
        // arrives with its own spec and changes nothing below this line.
        let (tx, events) = mock::channel();
        let mut source = MockSource::new();
        source.start(EventSink::new(tx));

        let machine = Machine::new(Timings::default(), Instant::now());
        let positions = window::load_positions(&handle);
        app.manage(Clawd::new(machine, source, positions));

        let window = window::create(&handle)?;
        window::place(&handle, &window);

        // Re-run the DPI snap when the display underneath the window changes
        // its scale factor, which includes being dragged onto another monitor
        // (§6.4).
        window.on_window_event({
            let handle = handle.clone();
            move |event| {
                if matches!(event, WindowEvent::ScaleFactorChanged { .. }) {
                    if let Some(window) = handle.get_webview_window(window::LABEL) {
                        window::apply_layout(&handle, &window);
                    }
                }
            }
        });

        tray::build(&handle)?;
        pump::spawn(handle.clone(), events);
        hit_test::spawn(handle.clone(), window);

        Ok(())
    });

    // The dev harness is registered only when it is compiled in, so a release
    // binary has no command to call (§8.3).
    #[cfg(feature = "devtools")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        ipc::hello,
        ipc::move_window,
        ipc::drag_finished,
        dev::dev_inject
    ]);
    #[cfg(not(feature = "devtools"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        ipc::hello,
        ipc::move_window,
        ipc::drag_finished
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running Clawd");
}
