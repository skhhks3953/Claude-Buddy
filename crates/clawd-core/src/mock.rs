//! The mock source, and the scripted run that exercises it.
//!
//! Dev mode is a source, not a UI toggle (§8.1). Synthetic events flow through
//! the real state machine, the real IPC and the real renderer, because a dev
//! panel that wrote straight to the view would let the state machine be wrong
//! while every state still looked correct.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::event::{EventKind, FailureKind, SessionEvent};
use crate::source::{EventSink, EventSource};
use crate::state::ClawdState;

/// The session id the harness pretends to be.
pub const PRIMARY_SESSION: &str = "mock-session-a";
/// A second session, for proving stickiness against a chatty neighbour (§8.2).
pub const SECOND_SESSION: &str = "mock-session-b";

/// A hand-driven event source.
#[derive(Default)]
pub struct MockSource {
    sink: Option<EventSink>,
    /// Handle for a scripted run in flight, so a second run can supersede it.
    script_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl MockSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push one event through, as the number keys do.
    pub fn inject(&self, event: SessionEvent) {
        if let Some(sink) = &self.sink {
            sink.emit(event);
        }
    }

    /// Play `script()` on a background thread in real time.
    ///
    /// This is what surfaces ugly transitions and label thrash, which are
    /// invisible when stepping one state at a time (§8.2).
    pub fn play_script(&self) {
        let Some(sink) = self.sink.clone() else {
            return;
        };
        let generation = self.script_generation.clone();
        let mine = generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

        thread::spawn(move || {
            for (delay, event) in script() {
                thread::sleep(delay);
                if generation.load(std::sync::atomic::Ordering::SeqCst) != mine {
                    return; // superseded by a newer run
                }
                sink.emit(event);
            }
        });
    }
}

impl EventSource for MockSource {
    fn start(&mut self, sink: EventSink) {
        self.sink = Some(sink);
    }
}

/// Convenience channel pairing for the shell.
pub fn channel() -> (Sender<SessionEvent>, Receiver<SessionEvent>) {
    mpsc::channel()
}

/// The event that puts Clawd directly into a given state, for keys 1-0 (§8.1).
///
/// Note that this still goes through the machine: pressing `9` asks for the
/// tool call that earns `LongTask`, and the pose only arrives once the timer
/// says so. The harness cannot set a state the machine would not produce.
pub fn event_for_state(state: ClawdState, session_id: &str) -> SessionEvent {
    let kind = match state {
        ClawdState::Idle => EventKind::SessionStarted,
        ClawdState::Thinking => EventKind::PromptSubmitted,
        ClawdState::Working | ClawdState::LongTask => EventKind::ToolStarted {
            name: "Edit".to_string(),
            target: Some("src/components/App.tsx".to_string()),
        },
        ClawdState::NeedsInput => EventKind::AttentionNeeded,
        ClawdState::NeedsPermission => EventKind::PermissionRequested {
            action: Some("edit".to_string()),
        },
        ClawdState::Success => EventKind::TurnFinished {
            duration: Some(Duration::from_secs(42)),
        },
        ClawdState::Failed => EventKind::TurnFailed {
            kind: Some(FailureKind::RateLimit),
        },
        ClawdState::Paused => EventKind::SessionEnded,
        ClawdState::Compacting => EventKind::CompactStarted,
    };
    SessionEvent::new(session_id, kind)
}

/// A realistic timed sequence: prompt, several tool calls, a permission
/// request, more work, success.
///
/// Returned as data rather than played directly, so tests can run it through
/// the machine with an injected clock and no sleeping.
pub fn script() -> Vec<(Duration, SessionEvent)> {
    let s = PRIMARY_SESSION;
    let ev = |kind| SessionEvent::new(s, kind);
    vec![
        (Duration::from_millis(0), ev(EventKind::SessionStarted)),
        (Duration::from_millis(900), ev(EventKind::PromptSubmitted)),
        (
            Duration::from_millis(1400),
            ev(EventKind::ToolStarted {
                name: "Read".to_string(),
                target: Some("src/components/Toolbar.tsx".to_string()),
            }),
        ),
        (Duration::from_millis(1100), ev(EventKind::ToolFinished)),
        (
            Duration::from_millis(600),
            ev(EventKind::ToolStarted {
                name: "Read".to_string(),
                target: Some("src/hooks/useHotkey.ts".to_string()),
            }),
        ),
        (Duration::from_millis(800), ev(EventKind::ToolFinished)),
        (
            Duration::from_millis(700),
            ev(EventKind::PermissionRequested {
                action: Some("edit".to_string()),
            }),
        ),
        (
            Duration::from_millis(3200),
            ev(EventKind::ToolStarted {
                name: "Edit".to_string(),
                target: Some("src/components/Toolbar.tsx".to_string()),
            }),
        ),
        (Duration::from_millis(1500), ev(EventKind::ToolFinished)),
        (
            Duration::from_millis(600),
            ev(EventKind::ToolStarted {
                name: "Bash".to_string(),
                target: Some("npm test".to_string()),
            }),
        ),
        // Long enough to earn the long-task pose and show the meter.
        (Duration::from_millis(13000), ev(EventKind::ToolFinished)),
        (
            Duration::from_millis(900),
            ev(EventKind::TurnFinished {
                duration: Some(Duration::from_secs(42)),
            }),
        ),
    ]
}
