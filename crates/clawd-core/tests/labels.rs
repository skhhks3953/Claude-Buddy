//! Label derivation (§5.1), and the things §5.3 forbids deriving.

use std::time::Duration;

use clawd_core::event::{EventKind, FailureKind};
use clawd_core::label;

fn tool(name: &str, target: Option<&str>) -> String {
    label::for_event(&EventKind::ToolStarted {
        name: name.to_string(),
        target: target.map(str::to_string),
    })
}

#[test]
fn tool_labels_match_the_spec_examples() {
    assert_eq!(tool("Edit", Some("src/components/App.tsx")), "Editing App.tsx");
    assert_eq!(tool("Bash", Some("npm test")), "Running npm test");
    assert_eq!(tool("Read", Some("src/hooks/useHotkey.ts")), "Reading useHotkey.ts");
}

/// A chip at 80px must not be mostly directory.
#[test]
fn paths_are_reduced_to_a_basename() {
    assert_eq!(tool("Edit", Some("/a/very/deep/path/Toolbar.tsx")), "Editing Toolbar.tsx");
    assert_eq!(tool("Edit", Some(r"C:\work\atlas\Toolbar.tsx")), "Editing Toolbar.tsx");
}

#[test]
fn unknown_tools_still_say_something_useful() {
    assert_eq!(tool("Grep", None), "Searching");
    assert_eq!(tool("SomeMcpTool", None), "Running SomeMcpTool");
    assert_eq!(tool("SomeMcpTool", Some("thing")), "SomeMcpTool thing");
}

#[test]
fn turn_duration_is_humanised() {
    let done = |secs| {
        label::for_event(&EventKind::TurnFinished {
            duration: Some(Duration::from_secs(secs)),
        })
    };
    assert_eq!(done(42), "Done in 42s");
    assert_eq!(done(185), "Done in 3m 05s");
    assert_eq!(label::for_event(&EventKind::TurnFinished { duration: None }), "Done");
}

#[test]
fn the_fixed_labels_read_as_specified() {
    assert_eq!(label::for_event(&EventKind::SessionStarted), "Ready");
    assert_eq!(label::for_event(&EventKind::AttentionNeeded), "Your turn");
    assert_eq!(label::for_event(&EventKind::CompactStarted), "Compacting…");
    assert_eq!(label::for_event(&EventKind::SessionEnded), "Paused");
    assert_eq!(
        label::for_event(&EventKind::PermissionRequested { action: Some("edit".into()) }),
        "Allow edit?"
    );
}

/// `Failed` carries a generic label plus the error kind where one is provided.
#[test]
fn failures_name_their_kind_when_they_have_one() {
    let failed = |kind| label::for_event(&EventKind::TurnFailed { kind });
    assert_eq!(failed(Some(FailureKind::RateLimit)), "Rate limited");
    assert_eq!(failed(Some(FailureKind::Other("Disk full".into()))), "Disk full");
    assert_eq!(failed(None), "Turn failed");
}

/// §5.3 is a standing constraint, not a preference: nothing in the event
/// vocabulary can express a progress percentage or a test-result count, so no
/// label can ever claim one. This test fails the moment someone adds a field
/// that would let it.
#[test]
fn no_label_can_claim_progress_it_does_not_have() {
    let every_kind = [
        EventKind::SessionStarted,
        EventKind::PromptSubmitted,
        EventKind::ToolStarted { name: "Bash".into(), target: Some("npm test".into()) },
        EventKind::ToolFinished,
        EventKind::ToolFailed { kind: Some(FailureKind::ToolError) },
        EventKind::PermissionRequested { action: Some("edit".into()) },
        EventKind::AttentionNeeded,
        EventKind::TurnFinished { duration: Some(Duration::from_secs(42)) },
        EventKind::TurnFailed { kind: Some(FailureKind::RateLimit) },
        EventKind::CompactStarted,
        EventKind::CompactFinished,
        EventKind::SessionEnded,
    ];

    for kind in every_kind {
        let text = label::for_event(&kind);
        assert!(!text.contains('%'), "{kind:?} produced a percentage: {text}");
        assert!(
            !text.to_lowercase().contains("tests failed"),
            "{kind:?} claimed parsed test results: {text}"
        );
    }
}

/// The chip is one line in a window 2.75 sprites wide. A Bash description can
/// be a whole command line, and uncapped it was clipped at both ends with no
/// ellipsis to say so.
#[test]
fn long_labels_are_capped_with_an_ellipsis() {
    let long = tool(
        "Bash",
        Some("npm run test:integration -- --runInBand --detectOpenHandles"),
    );
    assert!(
        long.chars().count() <= label::MAX_CHARS,
        "label was {} chars: {long}",
        long.chars().count()
    );
    assert!(long.ends_with('…'), "no ellipsis to show it was cut: {long}");
}

#[test]
fn a_label_that_fits_is_left_alone() {
    let short = tool("Bash", Some("npm test"));
    assert_eq!(short, "Running npm test");
    assert!(!short.ends_with('…'));
}

/// Truncation must split on a character boundary, not a byte one.
#[test]
fn capping_does_not_split_multibyte_characters() {
    let label = tool("Bash", Some(&"é".repeat(80)));
    assert!(label.chars().count() <= label::MAX_CHARS);
}

/// Every label the vocabulary can produce has to fit the chip.
#[test]
fn no_event_can_produce_an_oversized_label() {
    let monsters = [
        EventKind::ToolStarted {
            name: "A".repeat(90),
            target: Some("B".repeat(90)),
        },
        EventKind::PermissionRequested {
            action: Some("do something extraordinarily elaborate".into()),
        },
        EventKind::TurnFailed {
            kind: Some(FailureKind::Other("x".repeat(120))),
        },
    ];
    for kind in monsters {
        let text = label::for_event(&kind);
        assert!(
            text.chars().count() <= label::MAX_CHARS,
            "{kind:?} produced {} chars: {text}",
            text.chars().count()
        );
    }
}
