//! The state machine.
//!
//! This lives in Rust rather than React because the webview can be occluded,
//! throttled or suspended by the OS, and state must survive that (§3). It owns
//! the timers, is what later grows the action channel, and — the reason for
//! every design choice below — is testable without a browser.
//!
//! Time is injected rather than read from the clock: `apply` and `tick` both
//! take `now`. Tests drive the timers by arithmetic, never by sleeping.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::config::Timings;
use crate::event::{EventKind, SessionEvent};
use crate::label;
use crate::state::ClawdState;

/// Everything the renderer is given (§7.1): a state, plus label text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub state: ClawdState,
    pub label: String,
}

/// A pending state-driven transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Deadline {
    /// `Working` has run long enough to earn the long-task pose.
    LongTask,
    /// `Success` has been shown long enough; settle back to `Idle`.
    SuccessExpiry,
}

pub struct Machine {
    timings: Timings,
    state: ClawdState,
    label: String,
    /// The session whose events are currently being rendered (§4.2).
    owner: Option<String>,
    deadline: Option<(Instant, Deadline)>,
    /// When the owning session last said anything. Drives the watchdog.
    last_event_at: Instant,
}

impl Machine {
    pub fn new(timings: Timings, now: Instant) -> Self {
        Self {
            timings,
            state: ClawdState::Idle,
            label: label::READY.to_string(),
            owner: None,
            deadline: None,
            last_event_at: now,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            state: self.state,
            label: self.label.clone(),
        }
    }

    pub fn state(&self) -> ClawdState {
        self.state
    }

    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    pub fn timings(&self) -> Timings {
        self.timings
    }

    /// Swap the thresholds (the dev harness shrinks them, §8.2).
    ///
    /// Any pending deadline is rebased on the new thresholds so the change
    /// takes effect on the state already running, not just the next one.
    pub fn set_timings(&mut self, timings: Timings, now: Instant) {
        self.timings = timings;
        if let Some((_, deadline)) = self.deadline {
            self.arm(deadline, now);
        }
    }

    /// Feed an event in. Returns a snapshot only when something observable
    /// changed, so identical repeats do not thrash the IPC.
    pub fn apply(&mut self, event: &SessionEvent, now: Instant) -> Option<Snapshot> {
        if !self.may_own(&event.session_id) {
            return None;
        }
        self.owner = Some(event.session_id.clone());
        self.last_event_at = now;

        let next = next_state(&event.kind);
        let label = label::for_event(&event.kind);
        self.transition(next, label, now)
    }

    /// Advance the timers. Called on a low-frequency tick from the shell.
    pub fn tick(&mut self, now: Instant) -> Option<Snapshot> {
        // The watchdog outranks the state deadlines: if the session has gone
        // quiet there is nothing to promote it to (§4.3).
        if self.state.is_busy() && now.duration_since(self.last_event_at) >= self.timings.stale {
            return self.transition(ClawdState::Paused, label::STALLED.to_string(), now);
        }

        let (at, deadline) = self.deadline?;
        if now < at {
            return None;
        }

        match deadline {
            // The label is kept: whatever tool is running is still running,
            // and the long-task pose adds the elapsed meter, not new words.
            Deadline::LongTask => {
                let held = self.label.clone();
                self.transition(ClawdState::LongTask, held, now)
            }
            Deadline::SuccessExpiry => {
                self.transition(ClawdState::Idle, label::READY.to_string(), now)
            }
        }
    }

    /// When the next tick could possibly matter, so a shell that would rather
    /// sleep than poll can. Returns `None` when nothing is pending.
    pub fn next_wakeup(&self) -> Option<Instant> {
        let stale = self
            .state
            .is_busy()
            .then(|| self.last_event_at + self.timings.stale);
        let deadline = self.deadline.map(|(at, _)| at);
        match (stale, deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Ownership, and the stickiness that makes it safe (§2.2).
    ///
    /// A permission request fires once and then that session sits silent. A
    /// second, chattier session emitting tool events would overwrite it, and
    /// the one state the app exists to surface would be the one it reliably
    /// loses. So a different session takes over only when the current state is
    /// non-blocking.
    fn may_own(&self, session_id: &str) -> bool {
        match &self.owner {
            Some(owner) if owner != session_id => !self.state.is_blocking(),
            _ => true,
        }
    }

    fn transition(&mut self, next: ClawdState, label: String, now: Instant) -> Option<Snapshot> {
        let changed = next != self.state || label != self.label;
        let re_entered_working = next == ClawdState::Working && self.state == ClawdState::Working;

        self.state = next;
        self.label = label;

        // Re-arm even when the state did not change: a second tool call in a
        // row restarts the long-task clock, which is the honest reading of
        // "this one tool is taking a while".
        if changed || re_entered_working {
            match next {
                ClawdState::Working => self.arm(Deadline::LongTask, now),
                ClawdState::Success => self.arm(Deadline::SuccessExpiry, now),
                _ => self.deadline = None,
            }
        }

        changed.then(|| self.snapshot())
    }

    fn arm(&mut self, deadline: Deadline, now: Instant) {
        let after = match deadline {
            Deadline::LongTask => self.timings.long_task,
            Deadline::SuccessExpiry => self.timings.success_hold,
        };
        self.deadline = Some((now + after, deadline));
    }
}

/// The transition table (§4.1), kept as one free function so it reads as the
/// table it is.
///
/// `ToolFinished → Thinking` is deliberate: after a tool returns, Claude is
/// deciding what to do next, so the eyes-drifting-upward pose is truthful.
/// Returning to `Idle` between every tool call would make the pet flicker
/// constantly during normal work.
fn next_state(kind: &EventKind) -> ClawdState {
    match kind {
        EventKind::SessionStarted => ClawdState::Idle,
        EventKind::PromptSubmitted => ClawdState::Thinking,
        EventKind::ToolStarted { .. } => ClawdState::Working,
        EventKind::ToolFinished => ClawdState::Thinking,
        EventKind::ToolFailed { .. } => ClawdState::Failed,
        EventKind::PermissionRequested { .. } => ClawdState::NeedsPermission,
        EventKind::AttentionNeeded => ClawdState::NeedsInput,
        EventKind::TurnFinished { .. } => ClawdState::Success,
        EventKind::TurnFailed { .. } => ClawdState::Failed,
        EventKind::CompactStarted => ClawdState::Compacting,
        EventKind::CompactFinished => ClawdState::Thinking,
        EventKind::SessionEnded => ClawdState::Paused,
    }
}
