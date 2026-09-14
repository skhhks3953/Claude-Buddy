//! Translation from Claude Code hook payloads into Clawd's own vocabulary.
//!
//! This is the whole of what Clawd knows about Claude Code (§3.3). Everything
//! below this module speaks `SessionEvent`, so the state machine never learns
//! that hooks exist, and the mock stays a faithful stand-in for the real thing.
//!
//! It is parsing, not I/O, which is why it lives here rather than in the shell:
//! the table is assertable by `cargo test` at the workspace root, on a box with
//! no webview and no Claude Code installed.
//!
//! **What is read, and what is not.** The payload struct names the nine fields
//! Clawd needs. Serde drops everything else, so prompts, assistant messages,
//! tool output, transcript paths and file contents never enter the process at
//! all. That is enforced by a test, not merely intended.

use serde::Deserialize;
use serde_json::Value;

use crate::event::{EventKind, FailureKind, SessionEvent};

/// Longest a `ToolStarted` target may be.
///
/// `label::MAX_CHARS` is 30, so this is not about the chip — it is about not
/// carrying a whole command line through the channel and the IPC boundary to
/// be thrown away at the far end.
const MAX_TARGET: usize = 120;

/// The keys a tool's input might carry its subject under, most specific first.
///
/// Order settles the collisions: `Bash` has both `command` and `description`
/// and wants the command, giving `Running npm test` (§5.1); `Write` has both
/// `file_path` and `content` and must never reach `content`; `Grep` has
/// `pattern` and `path`.
const TARGET_KEYS: [&str; 7] = [
    "file_path",
    "notebook_path",
    "command",
    "pattern",
    "url",
    "description",
    "path",
];

/// A Claude Code hook payload, in the only fields Clawd reads.
///
/// Deliberately a flat struct with a `String` discriminant rather than an
/// enum tagged on `hook_event_name`: serde errors on an unknown variant, which
/// would turn every hook event Claude Code grows later into a parse failure.
/// Here an unrecognised name is simply not an event.
#[derive(Debug, Clone, Deserialize)]
pub struct HookPayload {
    pub hook_event_name: String,
    /// Required. This is the ownership key in `Machine::may_own`, and a
    /// defaulted empty id would silently merge distinct sessions into one.
    pub session_id: String,
    /// Present on subagent events. Its presence is what makes them droppable.
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<Value>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub error_type: Option<String>,
    #[serde(default)]
    pub how_started: Option<String>,
}

/// A body that was not a hook payload at all.
#[derive(Debug)]
pub struct HookError(serde_json::Error);

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not a hook payload: {}", self.0)
    }
}

impl std::error::Error for HookError {}

/// Parse a hook body and translate it.
///
/// `Ok(None)` means the payload was understood and is deliberately not an
/// event; `Err` means the bytes were not a hook payload. Keeping those apart
/// is what lets the listener count malformed input without also counting the
/// dozen events a turn that Clawd has no opinion about.
pub fn translate(body: &[u8]) -> Result<Option<SessionEvent>, HookError> {
    let payload: HookPayload = serde_json::from_slice(body).map_err(HookError)?;
    Ok(payload.to_event())
}

impl HookPayload {
    /// The event this payload means, if it means one.
    pub fn to_event(&self) -> Option<SessionEvent> {
        // Subagent events are dropped (§4). `Machine::may_own` keys on
        // `session_id` alone, and a subagent shares its parent's, so the
        // stickiness that protects a permission prompt from a chatty second
        // session gives no protection at all here: parallel agents would
        // become parallel writers to one chip, every `ToolStarted` would
        // re-arm the long-task deadline so the pose could never be earned,
        // and a subagent's `Stop` would render "Done" mid-turn. Folding them
        // in means keying ownership on `(session_id, agent_id)`, which is a
        // machine redesign rather than a translation detail.
        if self.agent_id.is_some() {
            return None;
        }

        let kind = self.kind()?;
        Some(SessionEvent::new(self.session_id.clone(), kind))
    }

    fn kind(&self) -> Option<EventKind> {
        match self.hook_event_name.as_str() {
            // A session restarting *from a compaction* is not a new session.
            // It lands immediately after `PostCompact`, so emitting
            // `SessionStarted` here would drop the pet to Idle in the middle
            // of a live turn. PreCompact/PostCompact own that whole arc.
            "SessionStart" if self.how_started.as_deref() == Some("compact") => None,
            "SessionStart" => Some(EventKind::SessionStarted),

            "UserPromptSubmit" => Some(EventKind::PromptSubmitted),

            "PreToolUse" => Some(EventKind::ToolStarted {
                name: self.tool_name.clone().unwrap_or_else(|| "tool".to_string()),
                target: self.target(),
            }),
            "PostToolUse" => Some(EventKind::ToolFinished),
            "PostToolUseFailure" => Some(EventKind::ToolFailed {
                kind: Some(FailureKind::ToolError),
            }),

            "PermissionRequest" => Some(EventKind::PermissionRequested {
                action: self.tool_name.as_deref().and_then(permission_verb),
            }),

            // `permission_prompt` is deliberately absent: `PermissionRequest`
            // covers it and carries `tool_name`, so it reads "Allow edit?",
            // where the notification has no tool name and reads "Permission
            // needed". Same state, different label — which `Machine` treats
            // as a change worth a snapshot, so the chip would visibly thrash.
            //
            // An unrecognised notification type is *not* treated as attention
            // either. `NeedsInput` is a blocking state, so a false positive is
            // sticky: it would lock the pet and shut every other session out
            // via `may_own` until this one spoke again.
            "Notification" => match self.notification_type.as_deref() {
                Some("idle_prompt" | "agent_needs_input") => Some(EventKind::AttentionNeeded),
                _ => None,
            },

            // Hooks carry no turn duration, and deriving one would mean
            // holding state across events for a number that is decoration.
            "Stop" => Some(EventKind::TurnFinished { duration: None }),
            "StopFailure" => Some(EventKind::TurnFailed {
                kind: failure_kind(self.error_type.as_deref()),
            }),

            "PreCompact" => Some(EventKind::CompactStarted),
            "PostCompact" => Some(EventKind::CompactFinished),

            "SessionEnd" => Some(EventKind::SessionEnded),

            // Forward compatibility: a hook event Clawd has never heard of is
            // not an error, it is simply not something the pet has a pose for.
            _ => None,
        }
    }

    /// What the tool is acting on, pulled out of a `tool_input` whose shape
    /// differs per tool.
    fn target(&self) -> Option<String> {
        let input = self.tool_input.as_ref()?.as_object()?;
        TARGET_KEYS
            .iter()
            // Non-string values are skipped rather than stringified: a
            // serialised array or object is noise in a 30-character chip.
            .find_map(|key| input.get(*key).and_then(Value::as_str))
            .map(str::trim)
            .filter(|target| !target.is_empty())
            .map(truncate)
    }
}

/// The verb the permission chip asks about.
///
/// `label::for_event` renders this as `Allow {action}?`, so it has to be a
/// lowercase verb phrase and never a tool name — "Allow Task?" reads as
/// nonsense. An unmapped tool falls back to `None`, whose existing label,
/// "Permission needed", already reads well.
fn permission_verb(tool: &str) -> Option<String> {
    let verb = match tool {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "edit",
        "Read" | "Glob" | "Grep" => "read",
        "Bash" | "BashOutput" | "KillShell" => "run",
        "WebFetch" | "WebSearch" => "web access",
        name if name.starts_with("mcp__") => "MCP access",
        _ => return None,
    };
    Some(verb.to_string())
}

/// Why a turn failed, where the hook says.
///
/// `FailureKind::describe` returns `Other`'s payload verbatim as UI copy, so
/// these are capitalised to match the built-in arms and kept well short of
/// `label::MAX_CHARS`. `Timeout` and `Cancelled` have no hook that reports
/// them — nothing in the vocabulary distinguishes a user pressing Esc — so
/// they stay reachable only from the mock rather than being invented here.
fn failure_kind(error_type: Option<&str>) -> Option<FailureKind> {
    match error_type? {
        "rate_limit" => Some(FailureKind::RateLimit),
        "overloaded" => Some(FailureKind::Other("Overloaded".to_string())),
        "authentication_failed" => Some(FailureKind::Other("Auth failed".to_string())),
        "server_error" => Some(FailureKind::Other("Server error".to_string())),
        "max_output_tokens" => Some(FailureKind::Other("Output limit".to_string())),
        // Including the literal "unknown", which carries no more information
        // than the generic "Turn failed" label already does.
        _ => None,
    }
}

/// Bound a target on a character boundary, never a byte one.
fn truncate(target: &str) -> String {
    if target.chars().count() <= MAX_TARGET {
        return target.to_string();
    }
    target.chars().take(MAX_TARGET).collect()
}
