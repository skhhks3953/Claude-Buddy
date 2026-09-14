//! Shared shell state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use clawd_core::layout::Layout;
use clawd_core::machine::Machine;
#[cfg(feature = "devtools")]
use clawd_core::mock::MockSource;

use crate::hook::HookSource;
use crate::position::PositionStore;

pub struct Clawd {
    /// The state machine. Owned here rather than in the webview, which the OS
    /// is free to occlude, throttle or suspend (§3).
    pub machine: Mutex<Machine>,
    /// The hook listener, and so the only thing that observes Claude Code.
    ///
    /// Held here for its lifetime as much as for its API: the source owns an
    /// `EventSink`, and dropping the last one closes the channel and ends the
    /// pump thread. That lifetime is the whole reason it is a field, so
    /// nothing reads it and nothing is expected to.
    #[allow(dead_code)]
    pub hooks: HookSource,
    /// The dev harness's source. Absent from release binaries rather than
    /// merely hidden (§8.3) — `hooks` now carries the channel, so there is no
    /// longer a reason for this to survive into one.
    #[cfg(feature = "devtools")]
    pub mock: MockSource,
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
    #[cfg(feature = "devtools")]
    pub fn new(
        machine: Machine,
        hooks: HookSource,
        positions: PositionStore,
        mock: MockSource,
    ) -> Self {
        Self {
            mock,
            ..Self::build(machine, hooks, positions)
        }
    }

    #[cfg(not(feature = "devtools"))]
    pub fn new(machine: Machine, hooks: HookSource, positions: PositionStore) -> Self {
        Self::build(machine, hooks, positions)
    }

    /// Everything both constructors share. Split so the dev-only field is the
    /// only difference between them.
    fn build(machine: Machine, hooks: HookSource, positions: PositionStore) -> Self {
        Self {
            machine: Mutex::new(machine),
            hooks,
            #[cfg(feature = "devtools")]
            mock: MockSource::new(),
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
