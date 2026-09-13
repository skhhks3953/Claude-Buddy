//! The adapter boundary.
//!
//! v1 has one implementation, `MockSource`. `HookSource` arrives with its own
//! spec (§10) and translates `PreToolUse` into `ToolStarted` without the state
//! machine ever learning that Claude Code exists.

use std::sync::mpsc::Sender;

use crate::event::SessionEvent;

/// Where a source pushes events.
#[derive(Clone)]
pub struct EventSink(Sender<SessionEvent>);

impl EventSink {
    pub fn new(tx: Sender<SessionEvent>) -> Self {
        Self(tx)
    }

    /// Emit an event. A closed channel means the app is shutting down, which
    /// is not a source's problem to report.
    pub fn emit(&self, event: SessionEvent) {
        let _ = self.0.send(event);
    }
}

/// A producer of session events.
pub trait EventSource: Send {
    /// Begin producing. Returns immediately; delivery is via the sink.
    fn start(&mut self, sink: EventSink);

    /// Stop producing. Sources that hold no resources need not override.
    fn stop(&mut self) {}

    /// The action channel, once one exists.
    ///
    /// v1 is a pure observer (§2.3): hooks are an observation channel and
    /// there is no mechanism to reach into a running session and pause it or
    /// answer a pending prompt. This hook is here because actions turn a
    /// one-way stream into request/response, which is cheap to leave room for
    /// now and expensive to retrofit.
    fn actions(&self) -> Option<&dyn ActionChannel> {
        None
    }
}

/// Reserved for a later spec. Nothing in v1 implements it.
pub trait ActionChannel: Send + Sync {}
