//! The ten states Clawd can be in.

use serde::{Deserialize, Serialize};

/// Every state the pet can render.
///
/// `LongTask` is a state in its own right rather than a flag on `Working`
/// because the renderer must stay dumb (§7.1) and the pose genuinely changes:
/// paw-drumming gives way to a slower bob plus the elapsed meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClawdState {
    Idle,
    Thinking,
    Working,
    NeedsInput,
    NeedsPermission,
    Success,
    Failed,
    Paused,
    LongTask,
    Compacting,
}

impl ClawdState {
    /// States that are waiting on the human.
    ///
    /// These are sticky: a chattier second session must not be able to
    /// overwrite the one state the app exists to surface (§2.2).
    pub fn is_blocking(self) -> bool {
        matches!(self, Self::NeedsInput | Self::NeedsPermission | Self::Failed)
    }

    /// States that claim work is in flight, and so are subject to the
    /// staleness watchdog (§4.3). Silent-but-wrong is worse than silent.
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Self::Thinking | Self::Working | Self::LongTask | Self::Compacting
        )
    }

    /// Stable identifier, matching the `camelCase` wire form the webview sees.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thinking => "thinking",
            Self::Working => "working",
            Self::NeedsInput => "needsInput",
            Self::NeedsPermission => "needsPermission",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Paused => "paused",
            Self::LongTask => "longTask",
            Self::Compacting => "compacting",
        }
    }

    /// Every state, in the order the dev harness binds them to keys 1-0 (§8.1).
    pub const ALL: [ClawdState; 10] = [
        Self::Idle,
        Self::Thinking,
        Self::Working,
        Self::NeedsInput,
        Self::NeedsPermission,
        Self::Success,
        Self::Failed,
        Self::Paused,
        Self::LongTask,
        Self::Compacting,
    ];
}
