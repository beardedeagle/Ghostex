//! The manager voice agent and Grok dictation: a spoken conversation about the user's agent
//! sessions (`/api/voice`, conversation mode) and the chat composer's dictation microphone
//! (`/api/voice?mode=dictation`), both relayed by gxserver to xAI or OpenAI with the keys kept on
//! this computer. Client contract: docs/2026-10-20/voice-mode/voice-agent-client-contract.md.

mod announcements;
mod conversation;
mod dictation;
mod http;
mod keys;
mod loopback;
mod prompt;
mod provider;
mod registry;
mod sessions;
mod socket;
mod tools;

pub(crate) use http::{handle_read_voice_agent_http, handle_set_voice_agent_key_http};
pub(crate) use socket::handle_voice;

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
