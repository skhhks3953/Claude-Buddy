//! The transition table of §4.1, asserted entry by entry.

use std::time::{Duration, Instant};

use clawd_core::event::{EventKind, FailureKind};
use clawd_core::machine::Machine;
use clawd_core::{ClawdState, SessionEvent, Timings};

const S: &str = "session-1";

fn machine() -> (Machine, Instant) {
    let now = Instant::now();
    (Machine::new(Timings::default(), now), now)
}

fn apply(m: &mut Machine, now: Instant, kind: EventKind) -> ClawdState {
    m.apply(&SessionEvent::new(S, kind), now);
    m.state()
}

#[test]
fn every_event_lands_on_its_specified_state() {
    let cases: Vec<(EventKind, ClawdState)> = vec![
        (EventKind::SessionStarted, ClawdState::Idle),
        (EventKind::PromptSubmitted, ClawdState::Thinking),
        (
            EventKind::ToolStarted {
                name: "Edit".into(),
                target: Some("App.tsx".into()),
            },
            ClawdState::Working,
        ),
        (EventKind::ToolFinished, ClawdState::Thinking),
        (EventKind::ToolFailed { kind: None }, ClawdState::Failed),
        (
            EventKind::PermissionRequested { action: None },
            ClawdState::NeedsPermission,
        ),
        (EventKind::AttentionNeeded, ClawdState::NeedsInput),
        (
            EventKind::TurnFinished { duration: None },
            ClawdState::Success,
        ),
        (EventKind::TurnFailed { kind: None }, ClawdState::Failed),
        (EventKind::CompactStarted, ClawdState::Compacting),
        (EventKind::CompactFinished, ClawdState::Thinking),
        (EventKind::SessionEnded, ClawdState::Paused),
    ];

    for (kind, expected) in cases {
        // A fresh machine per case, so each row is read on its own terms.
        let (mut m, now) = machine();
        // Blocking states are sticky, so start each case somewhere neutral.
        apply(&mut m, now, EventKind::PromptSubmitted);
        let got = apply(&mut m, now, kind.clone());
        assert_eq!(got, expected, "{kind:?} should produce {expected:?}");
    }
}

/// After a tool returns, Claude is deciding what to do next, so the
/// eyes-drifting-upward pose is truthful. Idle between every tool call would
/// make the pet flicker constantly during normal work (§4.1).
#[test]
fn tool_finished_returns_to_thinking_not_idle() {
    let (mut m, now) = machine();
    apply(&mut m, now, EventKind::PromptSubmitted);
    apply(
        &mut m,
        now,
        EventKind::ToolStarted {
            name: "Read".into(),
            target: Some("a.rs".into()),
        },
    );
    assert_eq!(apply(&mut m, now, EventKind::ToolFinished), ClawdState::Thinking);
}

#[test]
fn compact_round_trip_lands_on_thinking() {
    let (mut m, now) = machine();
    apply(&mut m, now, EventKind::PromptSubmitted);
    assert_eq!(
        apply(&mut m, now, EventKind::CompactStarted),
        ClawdState::Compacting
    );
    assert_eq!(
        apply(&mut m, now, EventKind::CompactFinished),
        ClawdState::Thinking
    );
}

#[test]
fn a_failure_kind_reaches_the_label() {
    let (mut m, now) = machine();
    let snapshot = m
        .apply(
            &SessionEvent::new(
                S,
                EventKind::TurnFailed {
                    kind: Some(FailureKind::RateLimit),
                },
            ),
            now,
        )
        .expect("state changed");
    assert_eq!(snapshot.state, ClawdState::Failed);
    assert_eq!(snapshot.label, "Rate limited");
}

/// Identical repeats must not thrash the IPC.
#[test]
fn an_unchanged_state_emits_no_snapshot() {
    let (mut m, now) = machine();
    let first = m.apply(&SessionEvent::new(S, EventKind::PromptSubmitted), now);
    assert!(first.is_some());
    let second = m.apply(
        &SessionEvent::new(S, EventKind::PromptSubmitted),
        now + Duration::from_secs(1),
    );
    assert!(second.is_none(), "repeat of the same event changed nothing");
}

/// Same state, different label — the chip must still be told.
#[test]
fn a_changed_label_emits_a_snapshot_even_within_one_state() {
    let (mut m, now) = machine();
    m.apply(
        &SessionEvent::new(
            S,
            EventKind::ToolStarted {
                name: "Edit".into(),
                target: Some("App.tsx".into()),
            },
        ),
        now,
    );
    let snapshot = m
        .apply(
            &SessionEvent::new(
                S,
                EventKind::ToolStarted {
                    name: "Bash".into(),
                    target: Some("npm test".into()),
                },
            ),
            now + Duration::from_secs(1),
        )
        .expect("label changed");
    assert_eq!(snapshot.state, ClawdState::Working);
    assert_eq!(snapshot.label, "Running npm test");
}
