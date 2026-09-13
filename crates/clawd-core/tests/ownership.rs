//! Ownership and sticky blocking states (§2.2, §4.2).
//!
//! The failure mode these guard against: a permission request fires once and
//! then that session sits silent, while a second, chattier session emits tool
//! events. Without stickiness the one state the app exists to surface is the
//! one it reliably loses.

use std::time::{Duration, Instant};

use clawd_core::event::EventKind;
use clawd_core::machine::Machine;
use clawd_core::{ClawdState, SessionEvent, Timings};

const A: &str = "session-a";
const B: &str = "session-b";

fn tool(name: &str) -> EventKind {
    EventKind::ToolStarted {
        name: name.to_string(),
        target: Some("src/App.tsx".to_string()),
    }
}

#[test]
fn a_chatty_neighbour_cannot_steal_a_permission_prompt() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);

    m.apply(
        &SessionEvent::new(A, EventKind::PermissionRequested { action: Some("edit".into()) }),
        now,
    );
    assert_eq!(m.state(), ClawdState::NeedsPermission);

    // B talks over it, repeatedly. None of it lands.
    for i in 1..=5 {
        let t = now + Duration::from_millis(200 * i);
        assert!(m.apply(&SessionEvent::new(B, tool("Read")), t).is_none());
        assert!(m.apply(&SessionEvent::new(B, EventKind::ToolFinished), t).is_none());
    }

    assert_eq!(m.state(), ClawdState::NeedsPermission);
    assert_eq!(m.owner(), Some(A));
}

#[test]
fn needs_input_and_failed_are_sticky_too() {
    for blocking in [
        EventKind::AttentionNeeded,
        EventKind::TurnFailed { kind: None },
    ] {
        let now = Instant::now();
        let mut m = Machine::new(Timings::default(), now);
        m.apply(&SessionEvent::new(A, blocking), now);
        let held = m.state();
        assert!(held.is_blocking());

        assert!(m
            .apply(&SessionEvent::new(B, tool("Bash")), now + Duration::from_secs(1))
            .is_none());
        assert_eq!(m.state(), held);
    }
}

/// The owner is never locked out of its own blocking state — answering the
/// prompt and carrying on must work.
#[test]
fn the_owning_session_can_leave_its_own_blocking_state() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);

    m.apply(&SessionEvent::new(A, EventKind::PermissionRequested { action: None }), now);
    m.apply(&SessionEvent::new(A, tool("Edit")), now + Duration::from_secs(3));

    assert_eq!(m.state(), ClawdState::Working);
}

/// Non-blocking states are last-writer-wins, as specified.
#[test]
fn ownership_moves_freely_while_nothing_is_blocking() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);

    m.apply(&SessionEvent::new(A, EventKind::PromptSubmitted), now);
    assert_eq!(m.owner(), Some(A));

    m.apply(&SessionEvent::new(B, tool("Read")), now + Duration::from_secs(1));
    assert_eq!(m.owner(), Some(B));
    assert_eq!(m.state(), ClawdState::Working);
}

/// Once the blocking state is cleared by its owner, the neighbour can take
/// over again — stickiness holds the state, it does not pin the session
/// forever.
#[test]
fn stickiness_releases_once_the_blocking_state_clears() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);

    m.apply(&SessionEvent::new(A, EventKind::AttentionNeeded), now);
    assert!(m.apply(&SessionEvent::new(B, tool("Read")), now).is_none());

    m.apply(&SessionEvent::new(A, EventKind::PromptSubmitted), now + Duration::from_secs(2));
    m.apply(&SessionEvent::new(B, tool("Read")), now + Duration::from_secs(3));

    assert_eq!(m.owner(), Some(B));
    assert_eq!(m.state(), ClawdState::Working);
}
