//! The scripted run (§8.2), replayed through the real machine on an injected
//! clock. Stepping one state at a time hides exactly the bugs this catches:
//! ugly transitions, and label thrash.

use std::time::{Duration, Instant};

use clawd_core::machine::Machine;
use clawd_core::{mock, ClawdState, Snapshot, Timings};

/// Replay the script, ticking at the same 250ms cadence the shell uses, and
/// collect every snapshot the machine chose to emit.
fn replay() -> Vec<(Duration, Snapshot)> {
    let start = Instant::now();
    let mut m = Machine::new(Timings::default(), start);
    let tick = Duration::from_millis(250);

    let mut schedule: Vec<(Duration, _)> = Vec::new();
    let mut at = Duration::ZERO;
    for (delay, event) in mock::script() {
        at += delay;
        schedule.push((at, event));
    }
    let end = at + Duration::from_secs(8);

    let mut out = Vec::new();
    let mut pending = schedule.into_iter().peekable();
    let mut elapsed = Duration::ZERO;

    while elapsed <= end {
        while pending.peek().is_some_and(|(t, _)| *t <= elapsed) {
            let (t, event) = pending.next().unwrap();
            if let Some(s) = m.apply(&event, start + t) {
                out.push((t, s));
            }
        }
        if let Some(s) = m.tick(start + elapsed) {
            out.push((elapsed, s));
        }
        elapsed += tick;
    }
    out
}

fn states(run: &[(Duration, Snapshot)]) -> Vec<ClawdState> {
    run.iter().map(|(_, s)| s.state).collect()
}

#[test]
fn the_run_ends_ready_after_a_success() {
    let run = replay();
    let seen = states(&run);
    let tail = &seen[seen.len() - 2..];
    assert_eq!(tail, [ClawdState::Success, ClawdState::Idle]);
    assert_eq!(run.last().unwrap().1.label, "Ready");
}

#[test]
fn the_run_visits_the_states_a_real_turn_would() {
    let seen = states(&replay());
    for expected in [
        ClawdState::Thinking,
        ClawdState::Working,
        ClawdState::NeedsPermission,
        ClawdState::LongTask,
        ClawdState::Success,
    ] {
        assert!(seen.contains(&expected), "run never reached {expected:?}: {seen:?}");
    }
}

/// The permission prompt sits for 3.2s in the script. Nothing may displace it,
/// and the watchdog must not touch it.
#[test]
fn the_permission_prompt_is_held_for_its_whole_duration() {
    let run = replay();
    let start = run
        .iter()
        .position(|(_, s)| s.state == ClawdState::NeedsPermission)
        .expect("permission requested");
    let next_change = run[start + 1].0;
    let held = next_change - run[start].0;
    assert!(
        held >= Duration::from_secs(3),
        "permission prompt was displaced after {held:?}"
    );
}

/// Label thrash: nothing should flip more than once per 250ms tick, and no
/// state should be emitted twice in a row.
#[test]
fn the_run_does_not_thrash() {
    let run = replay();
    for pair in run.windows(2) {
        assert_ne!(
            pair[0].1, pair[1].1,
            "the same snapshot was emitted twice in a row"
        );
    }
}

/// Working → LongTask must be reached by the timer, not by an event: nothing
/// in the script announces "this is taking a while" (§4).
#[test]
fn long_task_is_earned_by_the_clock() {
    let run = replay();
    let (at, snapshot) = run
        .iter()
        .find(|(_, s)| s.state == ClawdState::LongTask)
        .expect("long task reached");
    let started = run
        .iter()
        .rev()
        .find(|(t, s)| t < at && s.state == ClawdState::Working)
        .expect("working before it");
    assert!(*at - started.0 >= Duration::from_secs(10));
    assert_eq!(snapshot.label, "Running npm test");
}
