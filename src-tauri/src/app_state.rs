//! Shared shell state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use clawd_core::layout::Layout;
use clawd_core::machine::Machine;
use clawd_core::mock::MockSource;

use crate::position::PositionStore;

pub struct Clawd {
    /// The state machine. Owned here rather than in the webview, which the OS
    /// is free to occlude, throttle or suspend (§3).
    pub machine: Mutex<Machine>,
    /// The event source.
    ///
    /// Nothing reads this in a release build, because v1 has no `HookSource`
    /// and so nothing to inject — only the dev harness calls into it. It is
    /// still not dead: the source owns the `EventSink`, and dropping it would
    /// close the channel and end the pump thread.
    #[cfg_attr(not(feature = "devtools"), allow(dead_code))]
    pub source: MockSource,
    pub positions: Mutex<PositionStore>,
    pub layout: Mutex<Layout>,
    /// Whether the window is currently accepting clicks (§6.1).
    interactive: AtomicBool,
    /// True while a drag is in flight, which suspends the cursor poll.
    dragging: AtomicBool,
    /// Sub-pixel remainder carried between drag deltas.
    ///
    /// On a 1.5× display a one-pixel move is 1.5 physical pixels; rounding
    /// each delta on its own would drift the window a third of the way behind
    /// the cursor over a long drag.
    pub drag_residual: Mutex<(f64, f64)>,
}

impl Clawd {
    pub fn new(machine: Machine, source: MockSource, positions: PositionStore) -> Self {
        Self {
            machine: Mutex::new(machine),
            source,
            positions: Mutex::new(positions),
            layout: Mutex::new(Layout::default()),
            // The window starts click-through; the poll turns it on when the
            // cursor arrives.
            interactive: AtomicBool::new(false),
            dragging: AtomicBool::new(false),
            drag_residual: Mutex::new((0.0, 0.0)),
        }
    }

    pub fn is_interactive(&self) -> bool {
        self.interactive.load(Ordering::Relaxed)
    }

    pub fn set_interactive(&self, value: bool) {
        self.interactive.store(value, Ordering::Relaxed);
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.load(Ordering::Relaxed)
    }

    pub fn set_dragging(&self, value: bool) {
        self.dragging.store(value, Ordering::Relaxed);
    }
}
