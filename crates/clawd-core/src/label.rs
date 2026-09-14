//! Label text derivation.
//!
//! Everything here comes from data a hook payload actually carries (§5.1).
//! Progress percentages and test-result counts are not derivable and are not
//! attempted (§5.3) — obtaining either means pattern-matching on tool output.

use std::time::Duration;

use crate::event::{EventKind, FailureKind};

/// Longest a label may be, in characters.
///
/// The chip is one line in a window 2.75 sprites wide. A Bash description can
/// be an entire command line, and an uncapped one is clipped at both ends with
/// no ellipsis to say so. Sized to fit the chip at the default sprite size.
pub const MAX_CHARS: usize = 30;

pub const READY: &str = "Ready";
pub const THINKING: &str = "Thinking…";
pub const YOUR_TURN: &str = "Your turn";
pub const COMPACTING: &str = "Compacting…";
pub const PAUSED: &str = "Paused";
/// The watchdog's wording. A session that went quiet mid-work is a different
/// thing from one that ended cleanly, even though both render as `Paused`.
pub const STALLED: &str = "Session stalled";

/// Label for an event that has just been accepted.
pub fn for_event(kind: &EventKind) -> String {
    truncate(raw_for_event(kind))
}

/// Cap a label to `MAX_CHARS`, ending it with an ellipsis so the reader can
/// see it was cut. Splits on a character boundary, never a byte one.
fn truncate(text: String) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let kept: String = text.chars().take(MAX_CHARS - 1).collect();
    format!("{}…", kept.trim_end())
}

fn raw_for_event(kind: &EventKind) -> String {
    match kind {
        EventKind::SessionStarted => READY.to_string(),
        EventKind::PromptSubmitted => THINKING.to_string(),
        EventKind::ToolStarted { name, target } => for_tool(name, target.as_deref()),
        EventKind::ToolFinished => THINKING.to_string(),
        EventKind::ToolFailed { kind } => failure(kind.as_ref(), "Tool failed"),
        EventKind::PermissionRequested { action } => match action {
            Some(action) => format!("Allow {action}?"),
            None => "Permission needed".to_string(),
        },
        EventKind::AttentionNeeded => YOUR_TURN.to_string(),
        EventKind::TurnFinished { duration } => match duration {
            Some(d) => format!("Done in {}", humanise(*d)),
            None => "Done".to_string(),
        },
        EventKind::TurnFailed { kind } => failure(kind.as_ref(), "Turn failed"),
        EventKind::CompactStarted => COMPACTING.to_string(),
        EventKind::CompactFinished => THINKING.to_string(),
        EventKind::SessionEnded => PAUSED.to_string(),
    }
}

/// Tool name plus its target, per §5.1: `Editing App.tsx`, `Running npm test`.
fn for_tool(name: &str, target: Option<&str>) -> String {
    let subject = target.map(basename);
    match (name, subject) {
        ("Edit" | "Write" | "NotebookEdit" | "MultiEdit", Some(file)) => format!("Editing {file}"),
        ("Read", Some(file)) => format!("Reading {file}"),
        // Bash carries a description rather than a path, so it is used whole.
        ("Bash", Some(_)) => format!("Running {}", target.unwrap_or_default()),
        ("Grep" | "Glob", _) => "Searching".to_string(),
        ("WebFetch" | "WebSearch", _) => "Searching the web".to_string(),
        (name, Some(file)) => format!("{name} {file}"),
        (name, None) => format!("Running {name}"),
    }
}

fn failure(kind: Option<&FailureKind>, fallback: &str) -> String {
    match kind {
        Some(kind) => kind.describe().to_string(),
        None => fallback.to_string(),
    }
}

/// Last path segment, so a chip at 80px is not mostly directory.
fn basename(target: &str) -> &str {
    target
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(target)
}

/// `42s`, `3m 05s`. Whole seconds — this is a glanceable chip, not a stopwatch.
fn humanise(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m {:02}s", secs / 60, secs % 60)
    }
}
