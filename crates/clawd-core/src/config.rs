//! Timer thresholds.

use std::time::Duration;

/// The three durations the state machine is built around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timings {
    /// How long `Working` runs before it is earned the `LongTask` pose.
    /// Nothing in Claude Code announces "this is taking a while" (§4).
    pub long_task: Duration,
    /// How long `Success` holds before settling back to `Idle`.
    pub success_hold: Duration,
    /// Quiet period after which any busy state drops to `Paused` (§4.3).
    /// If the agent is killed outright no end event fires, and sitting on
    /// `Working` for ten minutes is confidently reporting something that
    /// stopped.
    pub stale: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            long_task: Duration::from_secs(10),
            success_hold: Duration::from_secs(5),
            stale: Duration::from_secs(120),
        }
    }
}

impl Timings {
    /// Shrunk thresholds for the dev harness, so the long-task pose and the
    /// watchdog are testable without waiting around (§8.2).
    pub fn fast() -> Self {
        Self {
            long_task: Duration::from_secs(2),
            success_hold: Duration::from_secs(2),
            stale: Duration::from_secs(6),
        }
    }
}
