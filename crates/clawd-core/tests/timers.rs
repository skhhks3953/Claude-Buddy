//! The three timers (§4.1, §4.3), driven by an injected clock rather than by
//! sleeping — so these tests are fast and cannot flake on a loaded machine.

use std::time::{Duration, Instant};

use clawd_core::event::EventKind;
use clawd_core::machine::Machine;
use clawd_core::{ClawdState, SessionEvent, Timings};

const S: &str = "session-1";

fn tool() -> EventKind {
    EventKind::ToolStarted {
        name: "Bash".to_string(),
        target: Some("npm test".to_string()),
    }
}

#[test]
fn working_earns_long_task_after_ten_seconds() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, tool()), now);

    assert!(m.tick(now + Duration::from_secs(9)).is_none());
    assert_eq!(m.state(), ClawdState::Working);

    let snapshot = m.tick(now + Duration::from_secs(10)).expect("promoted");
    assert_eq!(snapshot.state, ClawdState::LongTask);
}

/// The long-task pose adds the elapsed meter, not new words: whatever tool is
/// running is still running (§5.3).
#[test]
fn long_task_keeps_the_working_label() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, tool()), now);

    let snapshot = m.tick(now + Duration::from_secs(11)).expect("promoted");
    assert_eq!(snapshot.label, "Running npm test");
}

/// A second tool call restarts the clock — "this one tool is taking a while"
/// is the honest reading.
#[test]
fn a_new_tool_call_restarts_the_long_task_clock() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, tool()), now);

    let t = now + Duration::from_secs(8);
    m.apply(
        &SessionEvent::new(
            S,
            EventKind::ToolStarted {
                name: "Read".into(),
                target: Some("src/main.rs".into()),
            },
        ),
        t,
    );

    assert!(m.tick(now + Duration::from_secs(12)).is_none());
    assert_eq!(m.state(), ClawdState::Working);
    assert_eq!(m.tick(t + Duration::from_secs(10)).unwrap().state, ClawdState::LongTask);
}

#[test]
fn success_settles_back_to_idle() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(
        &SessionEvent::new(S, EventKind::TurnFinished { duration: Some(Duration::from_secs(42)) }),
        now,
    );
    assert_eq!(m.state(), ClawdState::Success);

    assert!(m.tick(now + Duration::from_secs(4)).is_none());
    let snapshot = m.tick(now + Duration::from_secs(5)).expect("settled");
    assert_eq!(snapshot.state, ClawdState::Idle);
    assert_eq!(snapshot.label, "Ready");
}

/// If Claude Code is killed outright no end event fires, and Clawd would sit
/// on `Working` indefinitely — confidently reporting something that stopped
/// ten minutes ago (§4.3).
#[test]
fn a_silent_busy_session_drops_to_paused() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, tool()), now);

    let snapshot = m.tick(now + Duration::from_secs(120)).expect("watchdog fired");
    assert_eq!(snapshot.state, ClawdState::Paused);
    assert_eq!(snapshot.label, "Session stalled");
}

#[test]
fn the_watchdog_covers_every_busy_state() {
    for kind in [
        EventKind::PromptSubmitted,
        EventKind::CompactStarted,
        EventKind::ToolStarted { name: "Bash".into(), target: None },
    ] {
        let now = Instant::now();
        let mut m = Machine::new(Timings::default(), now);
        m.apply(&SessionEvent::new(S, kind), now);
        assert!(m.state().is_busy());
        assert_eq!(
            m.tick(now + Duration::from_secs(121)).map(|s| s.state),
            Some(ClawdState::Paused)
        );
    }
}

/// A blocking state is quiet by nature. The watchdog must not mistake a
/// permission prompt nobody has answered for a dead session.
#[test]
fn the_watchdog_leaves_blocking_states_alone() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, EventKind::PermissionRequested { action: None }), now);

    assert!(m.tick(now + Duration::from_secs(600)).is_none());
    assert_eq!(m.state(), ClawdState::NeedsPermission);
}

#[test]
fn activity_keeps_the_watchdog_at_bay() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, tool()), now);

    // A tool finishing every 60s, for ten minutes.
    for i in 1..=10 {
        let t = now + Duration::from_secs(60 * i);
        m.apply(&SessionEvent::new(S, EventKind::ToolFinished), t);
        assert!(m.tick(t + Duration::from_secs(1)).is_none());
    }
    assert_eq!(m.state(), ClawdState::Thinking);
}

#[test]
fn dev_timer_overrides_apply_to_the_state_already_running() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, tool()), now);

    m.set_timings(Timings::fast(), now + Duration::from_secs(1));
    let snapshot = m.tick(now + Duration::from_secs(3)).expect("promoted early");
    assert_eq!(snapshot.state, ClawdState::LongTask);
}

#[test]
fn next_wakeup_reports_the_nearer_of_the_two_clocks() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);

    assert_eq!(m.next_wakeup(), None, "idle has nothing pending");

    m.apply(&SessionEvent::new(S, tool()), now);
    assert_eq!(m.next_wakeup(), Some(now + Duration::from_secs(10)));

    m.tick(now + Duration::from_secs(10));
    // Long task has no deadline of its own; the watchdog is what remains.
    assert_eq!(m.next_wakeup(), Some(now + Duration::from_secs(120)));
}

/// `advance` is the shell's whole loop pass, so the orchestration decision is
/// testable here rather than buried in the untested shell.
///
/// Note on reach: today every *accepted* event re-arms or clears the deadline,
/// and events are only rejected in blocking states, which carry no deadline —
/// so a pass where both the event and a timer fire is not currently
/// constructible. Doing both is defensive: the moment a timer applies to a
/// state that can also receive a rejected event, one-or-the-other would
/// silently stop advancing the clock.
/// With no event, advance is exactly a tick.
#[test]
fn advance_with_no_event_still_promotes() {
    let now = Instant::now();
    let mut m = Machine::new(Timings::default(), now);
    m.apply(&SessionEvent::new(S, tool()), now);

    assert!(m.advance(None, now + Duration::from_secs(9)).is_none());
    assert_eq!(
        m.advance(None, now + Duration::from_secs(10)).map(|s| s.state),
        Some(ClawdState::LongTask)
    );
}
