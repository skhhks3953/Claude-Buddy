//! Clawd's native shell.
//!
//! Everything that has to survive the webview being occluded, throttled or
//! suspended lives on this side: the state machine, the timers, the window,
//! the tray, and the hook listener. React is a pure render target (§3).

mod app_state;
#[cfg(feature = "devtools")]
mod dev;
mod hit_test;
mod hook;
mod ipc;
mod platform;
mod position;
mod pump;
mod tray;
mod window;

use std::time::Instant;

use clawd_core::machine::Machine;
#[cfg(feature = "devtools")]
use clawd_core::mock::MockSource;
use clawd_core::source::{self, EventSink, EventSource};
use clawd_core::Timings;
use tauri::{Manager, WindowEvent};

use app_state::Clawd;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().setup(|app| {
        let handle = app.handle().clone();

        // The adapter boundary (§3.1). It held: `HookSource` landed without
        // anything below this line changing.
        //
        // Both sources feed one channel, because `EventSink` is `Clone`. That
        // beats a `Box<dyn EventSource>`, which the trait's `Send`-without-
        // `Sync` bound would force behind a mutex that buys nothing.
        let (tx, events) = source::channel();
        let sink = EventSink::new(tx);

        let mut hooks = hook::HookSource::new(hook::requested_port());
        hooks.start(sink.clone());
        match hooks.port() {
            Some(port) => hook::save_port(&handle, port),
            // Nothing is listening, so a stale hint file would send the
            // installer at a port no one is on.
            None => hook::clear_port(&handle),
        }

        #[cfg(feature = "devtools")]
        let mock = {
            let mut mock = MockSource::new();
            mock.start(sink.clone());
            mock
        };

        // The sources hold the live clones; this one has done its job.
        drop(sink);

        let machine = Machine::new(Timings::default(), Instant::now());
        let positions = window::load_positions(&handle);

        #[cfg(feature = "devtools")]
        app.manage(Clawd::new(machine, hooks, positions, mock));
        #[cfg(not(feature = "devtools"))]
        app.manage(Clawd::new(machine, hooks, positions));

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
