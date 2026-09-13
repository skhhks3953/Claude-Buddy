//! Clawd's session state machine, and the adapter boundary that feeds it.
//!
//! This crate deliberately has no dependency on Tauri, on a webview, or on any
//! UI at all. That is what makes the pipeline testable without a desktop, and
//! what keeps the renderer swappable (§3, §11).

pub mod config;
pub mod event;
pub mod label;
pub mod layout;
pub mod machine;
pub mod mock;
pub mod source;
pub mod state;

pub use config::Timings;
pub use event::{EventKind, FailureKind, SessionEvent};
pub use layout::{Layout, Rect};
pub use machine::{Machine, Snapshot};
pub use source::{EventSink, EventSource};
pub use state::ClawdState;
