//! The hook translation table, asserted row by row.
//!
//! The shape follows `transitions.rs` deliberately: the two tables are the
//! only places where Clawd decides what something means, and they should read
//! the same way.

use clawd_core::event::{EventKind, FailureKind};
use clawd_core::hook::translate;
use clawd_core::{label, ClawdState, Machine, SessionEvent, Timings};
use std::time::Instant;

/// Build a payload body from a set of fields, so each row reads as the JSON
/// Claude Code actually posts rather than as a struct literal.
fn body(fields: &str) -> String {
    format!(r#"{{"session_id":"s1",{fields}}}"#)
}

fn event(fields: &str) -> Option<SessionEvent> {
    translate(body(fields).as_bytes()).expect("well-formed payload")
}

fn kind(fields: &str) -> Option<EventKind> {
    event(fields).map(|e| e.kind)
}

#[test]
fn every_hook_event_lands_on_its_specified_kind() {
    let cases: Vec<(&str, EventKind)> = vec![
        (r#""hook_event_name":"SessionStart""#, EventKind::SessionStarted),
        (
            r#""hook_event_name":"SessionStart","how_started":"resume""#,
            EventKind::SessionStarted,
        ),
        (
            r#""hook_event_name":"UserPromptSubmit""#,
            EventKind::PromptSubmitted,
        ),
        (
            r#""hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{"file_path":"src/App.tsx"}"#,
            EventKind::ToolStarted {
                name: "Edit".into(),
                target: Some("src/App.tsx".into()),
            },
        ),
        (r#""hook_event_name":"PostToolUse""#, EventKind::ToolFinished),
        (
            r#""hook_event_name":"PostToolUseFailure""#,
            EventKind::ToolFailed {
                kind: Some(FailureKind::ToolError),
            },
        ),
        (
            r#""hook_event_name":"PermissionRequest","tool_name":"Edit""#,
            EventKind::PermissionRequested {
                action: Some("edit".into()),
            },
        ),
        (
            r#""hook_event_name":"Notification","notification_type":"idle_prompt""#,
            EventKind::AttentionNeeded,
        ),
        (
            r#""hook_event_name":"Notification","notification_type":"agent_needs_input""#,
            EventKind::AttentionNeeded,
        ),
        (
            r#""hook_event_name":"Stop""#,
            EventKind::TurnFinished { duration: None },
        ),
        (
            r#""hook_event_name":"StopFailure","error_type":"rate_limit""#,
            EventKind::TurnFailed {
                kind: Some(FailureKind::RateLimit),
            },
        ),
        (r#""hook_event_name":"PreCompact""#, EventKind::CompactStarted),
        (
            r#""hook_event_name":"PostCompact""#,
            EventKind::CompactFinished,
        ),
        (r#""hook_event_name":"SessionEnd""#, EventKind::SessionEnded),
    ];

    for (fields, expected) in cases {
        assert_eq!(kind(fields).as_ref(), Some(&expected), "for {fields}");
    }
}

#[test]
fn the_session_id_is_carried_through_untouched() {
    let event = event(r#""hook_event_name":"Stop""#).expect("an event");
    assert_eq!(event.session_id, "s1");
}

/// Everything Clawd deliberately has no opinion about.
#[test]
fn understood_but_not_an_event() {
    let cases = [
        // A session resuming from a compaction is not a new session. It
        // arrives just after PostCompact, so SessionStarted here would drop
        // the pet to Idle mid-turn.
        r#""hook_event_name":"SessionStart","how_started":"compact""#,
        // PermissionRequest covers this one and carries the tool name, so
        // emitting both would flip the chip between two labels.
        r#""hook_event_name":"Notification","notification_type":"permission_prompt""#,
        r#""hook_event_name":"Notification","notification_type":"agent_completed""#,
        // NeedsInput is sticky, so an unrecognised notification must not be
        // guessed into one.
        r#""hook_event_name":"Notification","notification_type":"something_new""#,
        r#""hook_event_name":"Notification""#,
        // Forward compatibility: a hook event that did not exist yet.
        r#""hook_event_name":"SomeFutureHook""#,
    ];

    for fields in cases {
        assert_eq!(kind(fields), None, "{fields} should not be an event");
    }
}

/// Subagent events are dropped wholesale — see the comment on `to_event`.
#[test]
fn subagent_events_are_dropped() {
    let cases = [
        r#""hook_event_name":"PreToolUse","tool_name":"Edit","agent_id":"a1""#,
        r#""hook_event_name":"PostToolUse","agent_id":"a1""#,
        // The one that would otherwise lie outright: a subagent finishing
        // would render "Done" while the main turn is still running.
        r#""hook_event_name":"Stop","agent_id":"a1","agent_type":"Explore""#,
    ];

    for fields in cases {
        assert_eq!(kind(fields), None, "{fields} is a subagent event");
    }
}

#[test]
fn the_target_comes_from_whichever_key_the_tool_uses() {
    let cases: Vec<(&str, Option<&str>)> = vec![
        (r#"{"file_path":"src/App.tsx"}"#, Some("src/App.tsx")),
        (r#"{"notebook_path":"notes.ipynb"}"#, Some("notes.ipynb")),
        (r#"{"command":"npm test"}"#, Some("npm test")),
        (r#"{"pattern":"TODO"}"#, Some("TODO")),
        (r#"{"url":"https://example.com"}"#, Some("https://example.com")),
        (r#"{"description":"check the build"}"#, Some("check the build")),
        (r#"{"path":"crates/"}"#, Some("crates/")),
        // Bash carries both, and the command is the informative half: §5.1
        // wants "Running npm test".
        (
            r#"{"command":"npm test","description":"run the suite"}"#,
            Some("npm test"),
        ),
        // Grep carries both, and the pattern is the subject.
        (r#"{"pattern":"TODO","path":"src/"}"#, Some("TODO")),
        // Write carries the whole file under `content`. file_path wins first,
        // and `content` is not a target key at all.
        (
            r#"{"file_path":"out.txt","content":"a whole file"}"#,
            Some("out.txt"),
        ),
        // Non-string values are skipped, not stringified.
        (r#"{"file_path":["a","b"],"command":"ls"}"#, Some("ls")),
        (r#"{"file_path":"   "}"#, None),
        (r#"{"file_path":""}"#, None),
        (r#"{"timeout":30}"#, None),
        (r#"{}"#, None),
    ];

    for (input, expected) in cases {
        let fields =
            format!(r#""hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{input}"#);
        let EventKind::ToolStarted { target, .. } = kind(&fields).expect("a tool event") else {
            panic!("expected ToolStarted for {input}");
        };
        assert_eq!(target.as_deref(), expected, "for {input}");
    }
}

#[test]
fn a_long_target_is_bounded_on_a_character_boundary() {
    let long = "é".repeat(400);
    let fields = format!(
        r#""hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":"{long}"}}"#
    );
    let EventKind::ToolStarted { target, .. } = kind(&fields).expect("a tool event") else {
        panic!("expected ToolStarted");
    };
    assert_eq!(target.expect("a target").chars().count(), 120);
}

/// A tool Clawd has never heard of still names itself.
#[test]
fn an_unknown_tool_keeps_its_name() {
    let fields = r#""hook_event_name":"PreToolUse","tool_name":"mcp__memory__search""#;
    let EventKind::ToolStarted { name, target } = kind(fields).expect("a tool event") else {
        panic!("expected ToolStarted");
    };
    assert_eq!(name, "mcp__memory__search");
    assert_eq!(target, None);
}

#[test]
fn every_permission_verb_reads_as_a_verb() {
    let cases: Vec<(&str, Option<&str>)> = vec![
        ("Edit", Some("edit")),
        ("Write", Some("edit")),
        ("MultiEdit", Some("edit")),
        ("NotebookEdit", Some("edit")),
        ("Read", Some("read")),
        ("Glob", Some("read")),
        ("Grep", Some("read")),
        ("Bash", Some("run")),
        ("BashOutput", Some("run")),
        ("KillShell", Some("run")),
        ("WebFetch", Some("web access")),
        ("WebSearch", Some("web access")),
        ("mcp__memory__search", Some("MCP access")),
        // "Allow Task?" reads as nonsense, so an unmapped tool falls back to
        // the generic label instead.
        ("Task", None),
    ];

    for (tool, expected) in cases {
        let fields = format!(r#""hook_event_name":"PermissionRequest","tool_name":"{tool}""#);
        let EventKind::PermissionRequested { action } = kind(&fields).expect("a permission event")
        else {
            panic!("expected PermissionRequested for {tool}");
        };
        assert_eq!(action.as_deref(), expected, "for {tool}");
    }
}

#[test]
fn every_stop_failure_reason_maps_or_falls_back() {
    let cases: Vec<(&str, Option<FailureKind>)> = vec![
        ("rate_limit", Some(FailureKind::RateLimit)),
        ("overloaded", Some(FailureKind::Other("Overloaded".into()))),
        (
            "authentication_failed",
            Some(FailureKind::Other("Auth failed".into())),
        ),
        (
            "server_error",
            Some(FailureKind::Other("Server error".into())),
        ),
        (
            "max_output_tokens",
            Some(FailureKind::Other("Output limit".into())),
        ),
        // "unknown" carries nothing the generic "Turn failed" does not.
        ("unknown", None),
        ("something_new", None),
    ];

    for (error_type, expected) in cases {
        let fields = format!(r#""hook_event_name":"StopFailure","error_type":"{error_type}""#);
        let EventKind::TurnFailed { kind } = kind(&fields).expect("a failure event") else {
            panic!("expected TurnFailed for {error_type}");
        };
        assert_eq!(kind, expected, "for {error_type}");
    }
}

/// `Timeout` and `Cancelled` are reachable only from the mock. Nothing in the
/// hook vocabulary distinguishes a user pressing Esc from a turn ending, and
/// inventing a mapping would be a guess rendered as fact.
#[test]
fn timeout_and_cancelled_have_no_hook_that_reports_them() {
    for error_type in ["timeout", "cancelled", "timed_out"] {
        let fields = format!(r#""hook_event_name":"StopFailure","error_type":"{error_type}""#);
        let EventKind::TurnFailed { kind } = kind(&fields).expect("a failure event") else {
            panic!("expected TurnFailed");
        };
        assert_eq!(kind, None, "{error_type} must not be guessed into a kind");
    }
}

#[test]
fn a_body_that_is_not_a_hook_payload_is_an_error() {
    assert!(translate(b"not json at all").is_err());
    assert!(translate(b"{").is_err());
    // Valid JSON, but missing the two required fields.
    assert!(translate(br#"{"hello":"world"}"#).is_err());
    // A hook event name but no session, which is the ownership key.
    assert!(translate(br#"{"hook_event_name":"Stop"}"#).is_err());
}

#[test]
fn unknown_fields_are_ignored_rather_than_rejected() {
    let extras: String = (0..20)
        .map(|i| format!(r#""future_field_{i}":"value","#))
        .collect();
    let body = format!(r#"{{{extras}"hook_event_name":"Stop","session_id":"s1"}}"#);
    let event = translate(body.as_bytes())
        .expect("extra fields are not an error")
        .expect("still an event");
    assert_eq!(event.kind, EventKind::TurnFinished { duration: None });
}

/// The privacy contract, in the spirit of the test that no label may claim a
/// test-failure count. Clawd names nine fields; serde drops the rest, so the
/// content of a prompt or a file never enters the process.
#[test]
fn nothing_a_payload_carries_in_passing_can_reach_the_label() {
    let body = br#"{
        "hook_event_name": "PreToolUse",
        "session_id": "s1",
        "tool_name": "Write",
        "prompt": "SECRET",
        "last_assistant_message": "SECRET",
        "tool_response": "SECRET",
        "transcript_path": "/home/SECRET/transcript.jsonl",
        "cwd": "/home/SECRET",
        "tool_input": { "file_path": "notes.txt", "content": "SECRET" }
    }"#;

    let event = translate(body).expect("well-formed").expect("an event");
    assert!(
        !format!("{event:?}").contains("SECRET"),
        "a dropped field reached the event: {event:?}"
    );
    assert!(
        !label::for_event(&event.kind).contains("SECRET"),
        "a dropped field reached the label"
    );
}

/// Locks in that `ToolFailed` and `CompactFinished` are no longer variants
/// nothing can produce. If a variant is added later, this fails until a hook
/// payload produces it or the variant is deliberately listed as mock-only.
#[test]
fn every_event_kind_is_reachable_from_some_hook_payload() {
    let payloads = [
        r#""hook_event_name":"SessionStart""#,
        r#""hook_event_name":"UserPromptSubmit""#,
        r#""hook_event_name":"PreToolUse","tool_name":"Edit""#,
        r#""hook_event_name":"PostToolUse""#,
        r#""hook_event_name":"PostToolUseFailure""#,
        r#""hook_event_name":"PermissionRequest","tool_name":"Edit""#,
        r#""hook_event_name":"Notification","notification_type":"idle_prompt""#,
        r#""hook_event_name":"Stop""#,
        r#""hook_event_name":"StopFailure""#,
        r#""hook_event_name":"PreCompact""#,
        r#""hook_event_name":"PostCompact""#,
        r#""hook_event_name":"SessionEnd""#,
    ];

    let produced: Vec<EventKind> = payloads.iter().filter_map(|f| kind(f)).collect();
    let seen = |d: &str| produced.iter().any(|k| format!("{k:?}").starts_with(d));

    for variant in [
        "SessionStarted",
        "PromptSubmitted",
        "ToolStarted",
        "ToolFinished",
        "ToolFailed",
        "PermissionRequested",
        "AttentionNeeded",
        "TurnFinished",
        "TurnFailed",
        "CompactStarted",
        "CompactFinished",
        "SessionEnded",
    ] {
        assert!(seen(variant), "no hook payload produces {variant}");
    }
}

/// Every state the ten poses cover is reachable from a real hook payload.
#[test]
fn a_hook_payload_can_reach_every_non_timed_state() {
    let cases: Vec<(&str, ClawdState)> = vec![
        (r#""hook_event_name":"SessionStart""#, ClawdState::Idle),
        (
            r#""hook_event_name":"UserPromptSubmit""#,
            ClawdState::Thinking,
        ),
        (
            r#""hook_event_name":"PreToolUse","tool_name":"Edit""#,
            ClawdState::Working,
        ),
        (
            r#""hook_event_name":"Notification","notification_type":"idle_prompt""#,
            ClawdState::NeedsInput,
        ),
        (
            r#""hook_event_name":"PermissionRequest","tool_name":"Edit""#,
            ClawdState::NeedsPermission,
        ),
        (r#""hook_event_name":"Stop""#, ClawdState::Success),
        (r#""hook_event_name":"StopFailure""#, ClawdState::Failed),
        (r#""hook_event_name":"SessionEnd""#, ClawdState::Paused),
        (r#""hook_event_name":"PreCompact""#, ClawdState::Compacting),
    ];

    for (fields, expected) in cases {
        let now = Instant::now();
        let mut m = Machine::new(Timings::default(), now);
        // Blocking states are sticky, so start each row somewhere neutral.
        m.apply(
            &SessionEvent::new("s1", EventKind::PromptSubmitted),
            now,
        );
        m.apply(&event(fields).expect("an event"), now);
        assert_eq!(m.state(), expected, "for {fields}");
    }
}

/// A label wider than the chip is clipped at both ends with no ellipsis to say
/// so, which is why `label` truncates. Assert no hook payload can outrun it.
#[test]
fn no_hook_derived_label_outruns_the_chip() {
    let long = "x".repeat(500);
    let payloads = vec![
        format!(
            r#""hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":"{long}"}}"#
        ),
        format!(r#""hook_event_name":"PreToolUse","tool_name":"{long}""#),
        format!(r#""hook_event_name":"StopFailure","error_type":"{long}""#),
    ];

    for fields in payloads {
        let kind = kind(&fields).expect("an event");
        let text = label::for_event(&kind);
        assert!(
            text.chars().count() <= label::MAX_CHARS,
            "label {text:?} is {} chars",
            text.chars().count()
        );
    }
}
