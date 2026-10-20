//! The manager voice agent and Grok dictation: a spoken conversation about the user's agent
//! sessions (`/api/voice`, conversation mode) and the chat composer's dictation microphone, whose
//! recordings gxserver keeps and transcribes (`recordings.rs`, `transcribe.rs`,
//! `recordings_http.rs`; the older streaming `/api/voice?mode=dictation` is `dictation.rs`), all
//! sent to xAI or OpenAI with the keys kept on this computer. Client contract: docs/2026-10-20/voice-mode/voice-agent-client-contract.md.

mod announcements;
mod conversation;
mod dictation;
mod http;
mod keys;
mod loopback;
mod memory;
mod prompt;
mod provider;
mod question_forms;
mod recordings;
mod recordings_http;
mod registry;
mod sessions;
mod settings_http;
mod socket;
mod tools;
mod transcribe;

pub(crate) use http::{
    handle_forget_voice_agent_memory_http, handle_read_voice_agent_http,
    handle_set_voice_agent_key_http,
};
pub(crate) use memory::recover_after_restart as recover_voice_agent_memory;
pub(crate) use recordings_http::handle_dictation_recordings_http;
pub(crate) use settings_http::handle_set_voice_agent_settings_http;
pub(crate) use socket::handle_voice;

/// A warning whose `error` (kept by the log, unlike a `message` detail) holds a provider's own
/// explanation, such as an HTTP 402 body. Never pass a key, audio or transcript text here.
fn warn_error(
    state: &crate::server::AppState,
    event: &str,
    error: &str,
    details: serde_json::Value,
) {
    let _ = state.logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error.to_string()),
        details: Some(details),
    });
}

/// A warning in gxserver's log (warnings are written whatever the diagnostic switches say). Never
/// pass a key, audio or transcript text here.
fn warn(state: &crate::server::AppState, event: &str, details: serde_json::Value) {
    let _ = state.logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}
