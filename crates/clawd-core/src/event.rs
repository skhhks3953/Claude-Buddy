//! Clawd's own event vocabulary.
//!
//! These are domain events, not hook events (§3.3). `HookSource` will one day
//! translate `PreToolUse` into `ToolStarted`; the state machine never learns
//! that Claude Code exists. That seam is what makes the integration swap a
//! config change rather than a rewrite, and what lets the mock be a faithful
//! stand-in.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// An event about one session.
///
/// Every event carries a `session_id`, so multi-session support later is an
/// additive change rather than a rewrite (§2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub session_id: String,
    pub kind: EventKind,
}

impl SessionEvent {
    pub fn new(session_id: impl Into<String>, kind: EventKind) -> Self {
        Self {
            session_id: session_id.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventKind {
    SessionStarted,
    PromptSubmitted,
    ToolStarted {
        /// Tool name as the agent knows it: `Edit`, `Bash`, `Read`, …
        name: String,
        /// What it is acting on — a file path, or a command description.
        target: Option<String>,
    },
    ToolFinished,
    ToolFailed {
        kind: Option<FailureKind>,
    },
    PermissionRequested {
        /// The verb being asked about, e.g. `edit`, so the chip can read
        /// "Allow edit?".
        action: Option<String>,
    },
    AttentionNeeded,
    TurnFinished {
        /// Wall time for the turn, where the source can supply it.
        duration: Option<Duration>,
    },
    TurnFailed {
        kind: Option<FailureKind>,
    },
    CompactStarted,
    CompactFinished,
    SessionEnded,
}

/// Why something failed, where the source knows.
///
/// Only ever used to colour the label — `Failed` carries a generic message
/// plus the kind where one is provided, such as a rate limit (§5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum FailureKind {
    RateLimit,
    Timeout,
    Cancelled,
    ToolError,
    Other(String),
}

impl FailureKind {
    /// A short human phrase. Deliberately generic: nothing here is parsed out
    /// of tool output, because that is screen-scraping in better clothes
    /// (§5.3).
    pub fn describe(&self) -> &str {
        match self {
            Self::RateLimit => "Rate limited",
            Self::Timeout => "Timed out",
            Self::Cancelled => "Cancelled",
            Self::ToolError => "Tool failed",
            Self::Other(detail) => detail,
        }
    }
}
