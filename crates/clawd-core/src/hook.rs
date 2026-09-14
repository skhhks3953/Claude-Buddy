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
//! **The field names here are the ones the running Claude Code binary emits**,
//! which are not in every case the ones its published reference lists. Where
//! the two disagree the binary wins, and the disagreements are noted inline —
//! an integration built on a field that does not exist fails silently, because
//! serde simply leaves the `Option` as `None` and every label falls back.
//!
//! **What is read, and what is not.** The payload struct names the fields
//! Clawd needs and `ToolInput` names the seven keys a target can come from.
//! Serde discards everything else as it parses, so prompts, assistant
//! messages, tool output, transcript paths and file contents are never
//! retained. That is enforced by a test, not merely intended.

use serde::Deserialize;

use crate::event::{EventKind, FailureKind, SessionEvent};

/// Longest a `ToolStarted` target may be.
///
/// `label::MAX_CHARS` is 30, so this is not about the chip — it is about not
/// carrying a whole command line through the channel and the IPC boundary to
/// be thrown away at the far end.
const MAX_TARGET: usize = 120;

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
    pub tool_input: Option<ToolInput>,
    #[serde(default)]
    pub notification_type: Option<String>,
    /// `StopFailure`'s reason. The field is `error`, not `error_type` — the
    /// published reference says otherwise and is wrong, and the difference is
    /// invisible at runtime because a missing field just means no reason.
    #[serde(default)]
    pub error: Option<String>,
    /// True when a tool call ended because the user pressed Esc. A deliberate
    /// interrupt is not a failure and must not paint the pet red.
    #[serde(default)]
    pub is_interrupt: Option<bool>,
    /// How the session began: `startup`, `resume`, `clear`, `compact`, `fork`.
    /// The field is `source`, not `how_started`, for the same reason as above.
    #[serde(default)]
    pub source: Option<String>,
}

/// The subject of a tool call, across the shapes different tools use.
///
/// Naming the seven keys rather than holding a whole `serde_json::Value` is
/// what makes the privacy claim true rather than aspirational: a `Write`'s
/// `content` is the file it is writing, and here it is skipped as it is
/// parsed instead of being allocated and then ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolInput {
    #[serde(default, deserialize_with = "string_or_none")]
    pub file_path: Option<String>,
    #[serde(default, deserialize_with = "string_or_none")]
    pub notebook_path: Option<String>,
    #[serde(default, deserialize_with = "string_or_none")]
    pub command: Option<String>,
    #[serde(default, deserialize_with = "string_or_none")]
    pub pattern: Option<String>,
    #[serde(default, deserialize_with = "string_or_none")]
    pub url: Option<String>,
    #[serde(default, deserialize_with = "string_or_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "string_or_none")]
    pub path: Option<String>,
}

/// Keep a value only when it is a string.
///
/// A target key can hold something other than a string — `MultiEdit` puts an
/// array under `edits`, and tools are free to reuse these names for anything.
/// Rejecting the whole payload over that would silently cost a state, and
/// stringifying it would put a serialised array in the chip. Neither is worth
/// it: the key simply does not contribute a target.
fn string_or_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(deserializer) {
        Ok(serde_json::Value::String(text)) => Some(text),
        _ => None,
    })
}

impl ToolInput {
    /// The subject, most specific key first.
    ///
    /// Order settles the collisions: `Bash` carries both `command` and
    /// `description` and wants the command, giving `Running npm test` (§5.1);
    /// `Grep` carries `pattern` and `path`; `Write` carries `file_path`
    /// alongside the file's whole contents, which are not a key here at all.
    fn target(&self) -> Option<&str> {
        [
            &self.file_path,
            &self.notebook_path,
            &self.command,
            &self.pattern,
            &self.url,
            &self.description,
            &self.path,
        ]
        .into_iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
    }
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
            "SessionStart" if self.source.as_deref() == Some("compact") => None,
            "SessionStart" => Some(EventKind::SessionStarted),

            "UserPromptSubmit" => Some(EventKind::PromptSubmitted),

            "PreToolUse" => Some(EventKind::ToolStarted {
                name: self.tool_name.clone().unwrap_or_else(|| "tool".to_string()),
                target: self.target(),
            }),
            "PostToolUse" => Some(EventKind::ToolFinished),

            // Esc is not a failure. The user interrupted the tool themselves,
            // they know they did it, and painting the pet red would be both
            // wrong and sticky. What is true afterwards is that Claude Code
            // has handed control back, which is precisely `AttentionNeeded`.
            "PostToolUseFailure" if self.is_interrupt == Some(true) => {
                Some(EventKind::AttentionNeeded)
            }
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
                kind: failure_kind(self.error.as_deref()),
            }),

            "PreCompact" => Some(EventKind::CompactStarted),
            "PostCompact" => Some(EventKind::CompactFinished),

            "SessionEnd" => Some(EventKind::SessionEnded),

            // Forward compatibility: a hook event Clawd has never heard of is
            // not an error, it is simply not something the pet has a pose for.
            _ => None,
        }
    }

    fn target(&self) -> Option<String> {
        self.tool_input
            .as_ref()
            .and_then(ToolInput::target)
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
/// The values are `StopFailure`'s `error` enum. `FailureKind::describe`
/// returns `Other`'s payload verbatim as UI copy, so these are capitalised to
/// match the built-in arms and kept well short of `label::MAX_CHARS`.
fn failure_kind(error: Option<&str>) -> Option<FailureKind> {
    let text = match error? {
        "rate_limit" => "",
        "overloaded" => "Overloaded",
        "authentication_failed" | "cloud_credential_error" => "Auth failed",
        "oauth_org_not_allowed" | "account_on_hold" | "billing_error" => "Account problem",
        "invalid_request" | "model_not_found" => "Request rejected",
        "server_error" => "Server error",
        "max_output_tokens" => "Output limit",
        // Including the literal "unknown", which carries no more information
        // than the generic "Turn failed" label already does.
        _ => return None,
    };
    if text.is_empty() {
        return Some(FailureKind::RateLimit);
    }
    Some(FailureKind::Other(text.to_string()))
}

/// Bound a target on a character boundary, never a byte one.
fn truncate(target: &str) -> String {
    if target.chars().count() <= MAX_TARGET {
        return target.to_string();
    }
    target.chars().take(MAX_TARGET).collect()
}
