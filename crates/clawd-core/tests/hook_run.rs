//! A real Claude Code turn, as hook payloads, replayed through the real
//! machine on an injected clock.
//!
//! The hook-world counterpart to `scripted_run.rs`. That one proves the mock
//! drives the machine sensibly; this one proves the *translation* does, which
//! is the only new thing between a hook payload and a pose.

use std::time::{Duration, Instant};

use clawd_core::hook::translate;
use clawd_core::machine::Machine;
use clawd_core::{ClawdState, Snapshot, Timings};

const SESSION: &str = "abc123";

/// Replay a timed list of raw hook bodies, ticking at the shell's 250ms
/// cadence, and collect every snapshot the machine chose to emit.
fn replay(script: Vec<(Duration, String)>, tail: Duration) -> Vec<(Duration, Snapshot)> {
    let start = Instant::now();
    let mut m = Machine::new(Timings::default(), start);
    let tick = Duration::from_millis(250);

    let end = script.last().map(|(t, _)| *t).unwrap_or_default() + tail;
    let mut out = Vec::new();
    let mut pending = script.into_iter().peekable();
    let mut elapsed = Duration::ZERO;

    while elapsed <= end {
        while pending.peek().is_some_and(|(t, _)| *t <= elapsed) {
            let (t, body) = pending.next().expect("peeked");
            // A body that does not translate is simply not an event, exactly
            // as the listener treats it.
            if let Ok(Some(event)) = translate(body.as_bytes()) {
                if let Some(s) = m.apply(&event, start + t) {
                    out.push((t, s));
                }
            }
        }
        if let Some(s) = m.tick(start + elapsed) {
            out.push((elapsed, s));
        }
        elapsed += tick;
    }
    out
}

fn at(secs: u64, fields: &str) -> (Duration, String) {
    (
        Duration::from_secs(secs),
        format!(r#"{{"session_id":"{SESSION}","hook_event_name":{fields}}}"#),
    )
}

fn states(run: &[(Duration, Snapshot)]) -> Vec<ClawdState> {
    run.iter().map(|(_, s)| s.state).collect()
}

fn labels(run: &[(Duration, Snapshot)]) -> Vec<String> {
    run.iter().map(|(_, s)| s.label.clone()).collect()
}

/// A turn that reads a file, asks permission to edit it, and finishes.
#[test]
fn an_ordinary_turn_reads_the_way_it_should() {
    let run = replay(
        vec![
            at(0, r#""SessionStart","source":"startup""#),
            at(1, r#""UserPromptSubmit""#),
            at(2, r#""PreToolUse","tool_name":"Read","tool_input":{"file_path":"src/App.tsx"}"#),
            at(3, r#""PostToolUse","tool_name":"Read""#),
            at(4, r#""PermissionRequest","tool_name":"Edit""#),
            at(9, r#""PreToolUse","tool_name":"Edit","tool_input":{"file_path":"src/App.tsx"}"#),
            at(10, r#""PostToolUse","tool_name":"Edit""#),
            at(11, r#""Stop""#),
        ],
        // Long enough for Success to settle back to Idle.
        Duration::from_secs(7),
    );

    // No leading Idle: the machine starts there, and an unchanged state
    // emits no snapshot, so `SessionStart` is correctly silent.
    assert_eq!(
        states(&run),
        vec![
            ClawdState::Thinking,
            ClawdState::Working,
            ClawdState::Thinking,
            ClawdState::NeedsPermission,
            ClawdState::Working,
            ClawdState::Thinking,
            ClawdState::Success,
            ClawdState::Idle,
        ]
    );

    assert_eq!(
        labels(&run),
        vec![
            "Thinking…",
            "Reading App.tsx",
            "Thinking…",
            "Allow edit?",
            "Editing App.tsx",
            "Thinking…",
            "Done",
            "Ready",
        ]
    );
}

/// The case the mock could never produce: `PreToolUse` fires, and then nothing
/// arrives at all until the build finishes four minutes later. A watchdog
/// sized for the mock's 13-second gaps would call this a stalled session and
/// be wrong every single time anyone ran a build.
#[test]
fn a_long_build_is_not_mistaken_for_a_stalled_session() {
    let run = replay(
        vec![
            at(0, r#""UserPromptSubmit""#),
            at(1, r#""PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo build"}"#),
            at(241, r#""PostToolUse","tool_name":"Bash""#),
            at(242, r#""Stop""#),
        ],
        Duration::from_secs(7),
    );

    assert!(
        !states(&run).contains(&ClawdState::Paused),
        "a four-minute build must not read as stalled: {:?}",
        labels(&run)
    );
    // It should have earned the long-task pose instead, which is precisely
    // what that state is for.
    assert!(
        states(&run).contains(&ClawdState::LongTask),
        "a four-minute build should earn the long-task pose: {:?}",
        states(&run)
    );
    assert_eq!(states(&run).last(), Some(&ClawdState::Idle));
}

/// The watchdog still has to fire when the session really is dead — killed
/// outright, so no end event ever comes.
#[test]
fn a_killed_session_still_drops_to_paused() {
    let run = replay(
        vec![
            at(0, r#""UserPromptSubmit""#),
            at(1, r#""PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo build"}"#),
        ],
        Duration::from_secs(700),
    );

    assert_eq!(states(&run).last(), Some(&ClawdState::Paused));
    assert_eq!(labels(&run).last().map(String::as_str), Some("Session stalled"));
}

/// Subagent traffic is dropped in translation, so a turn that fans out to
/// three parallel agents must look exactly like one that does not — and the
/// parent's long-task pose must still be earned, which it could not be if
/// every subagent tool call re-armed the deadline.
#[test]
fn parallel_subagents_do_not_reach_the_chip() {
    let mut script = vec![
        at(0, r#""UserPromptSubmit""#),
        at(1, r#""PreToolUse","tool_name":"Task","tool_input":{"description":"explore"}"#),
    ];

    // Three agents, each chattering through the whole of the parent's work.
    for i in 0..3 {
        for step in 0..12 {
            let t = 2 + step * 2;
            script.push((
                Duration::from_secs(t),
                format!(
                    r#"{{"session_id":"{SESSION}","agent_id":"agent-{i}","agent_type":"Explore",
                       "hook_event_name":"PreToolUse","tool_name":"Grep","tool_input":{{"pattern":"TODO"}}}}"#
                ),
            ));
        }
        // The one that would otherwise lie: an agent finishing mid-turn.
        script.push((
            Duration::from_secs(26),
            format!(
                r#"{{"session_id":"{SESSION}","agent_id":"agent-{i}","hook_event_name":"Stop"}}"#
            ),
        ));
    }
    script.push(at(30, r#""PostToolUse","tool_name":"Task""#));
    script.push(at(31, r#""Stop""#));
    script.sort_by_key(|(t, _)| *t);

    let run = replay(script, Duration::from_secs(7));

    assert_eq!(
        states(&run),
        vec![
            ClawdState::Thinking,
            ClawdState::Working,
            // Earned by the clock at ten seconds, and never reset by the
            // thirty-six subagent tool calls in between.
            ClawdState::LongTask,
            ClawdState::Thinking,
            ClawdState::Success,
            ClawdState::Idle,
        ]
    );
    // No subagent ever reached the label.
    assert!(
        !labels(&run).iter().any(|l| l == "Searching"),
        "a subagent's Grep reached the chip: {:?}",
        labels(&run)
    );
}

/// Compaction has to round-trip back into the turn, not out of it. The
/// `SessionStart` that Claude Code fires after compacting would otherwise drop
/// the pet to Idle in the middle of live work.
#[test]
fn compaction_returns_to_the_turn_rather_than_resetting_it() {
    let run = replay(
        vec![
            at(0, r#""UserPromptSubmit""#),
            at(1, r#""PreCompact","trigger":"auto""#),
            at(4, r#""PostCompact","trigger":"auto""#),
            at(4, r#""SessionStart","source":"compact""#),
            at(5, r#""PreToolUse","tool_name":"Edit","tool_input":{"file_path":"a.ts"}"#),
        ],
        Duration::from_secs(2),
    );

    assert_eq!(
        states(&run),
        vec![
            ClawdState::Thinking,
            ClawdState::Compacting,
            ClawdState::Thinking,
            ClawdState::Working,
        ],
        "labels were {:?}",
        labels(&run)
    );
}
