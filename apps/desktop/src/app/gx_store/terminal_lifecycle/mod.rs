//! Workspace terminal events and lifecycle requests the app used to hand to the QuickJS runtime
//! (`terminal-lifecycle-queue.ts`), performed in Rust. Per-concern files; this barrel stays thin.

pub(crate) mod chat_grid_claims;
mod close_after_done;
mod desktop;
mod group_sleep;
mod lifecycle_requests;
pub(crate) mod open_conversation;
mod runtime_actions;
pub(crate) mod session_calls;
mod session_edits;
pub(crate) mod shown_sessions;
pub(crate) mod shown_sessions_report;
pub(crate) mod terminal_events;
