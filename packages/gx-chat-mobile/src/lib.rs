//! UniFFI binding of the Ghostex chat core for the mobile app.
//!
//! The whole surface is [`MobileChatCore`] plus two free functions, with JSON strings at the boundary
//! (shapes: `packages/gx-chat-json`, and the doc comment in
//! `apps/mobile/app/modules/gx-chat-core/src/index.ts`). Nothing here decides anything about chat:
//! every call forwards to [`JsonChatCore`], which parses, calls the core, and serializes. The same
//! JSON layer also backs the WebAssembly build (`packages/gx-chat-wasm`).
//!
//! Every exported call catches a panic and answers `{"error": {"code": "panic", ...}}` rather than
//! unwinding into Swift or Kotlin. A core that panicked stays refused afterwards, because its state
//! may be half-applied; the desktop host disables a panicked chat the same way.

use ghostex_gx_chat_json::JsonChatCore;

uniffi::setup_scaffolding!();

/// One chat session's brain.
#[derive(uniffi::Object)]
pub struct MobileChatCore {
    inner: JsonChatCore,
}

#[uniffi::export]
impl MobileChatCore {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            inner: JsonChatCore::new(),
        }
    }

    /// Applies one event; returns the effects as a JSON array, or a JSON error.
    pub fn handle(&self, event_json: String, context_json: String) -> String {
        self.inner.handle(&event_json, &context_json)
    }

    /// The drained frame envelope as JSON, with `nextWakeMs` measured at `now_ms`.
    ///
    /// A negative `last_revision` means the host holds no document, so the snapshot ships.
    pub fn frame_at(&self, last_revision: i64, now_ms: f64) -> String {
        self.inner.frame_at(last_revision, now_ms)
    }

    /// Milliseconds until the earliest armed deadline, or `None` (also after a panic).
    pub fn next_wake_ms(&self) -> Option<u64> {
        self.inner.next_wake_ms()
    }

    /// The current publish revision (0 after a panic).
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }

    /// Makes the next frame ship every channel whole. Returns `{}` or a JSON error.
    pub fn forget_sent(&self) -> String {
        self.inner.forget_sent()
    }

    /// A fresh id from the core's request counter (0 after a panic, which the core never hands out).
    pub fn allocate_request_id(&self) -> u64 {
        self.inner.allocate_request_id()
    }

    /// One of the pure helpers (`composerReferences`, `composerKeyIntent`, `referenceMenu`,
    /// `transcriptMenu`, `sendBlockedToast`, `insertAnswerAttachments`, `removeChatReference`,
    /// `renderVisual`, `speakableSegments`, `speakableMarkdown`), answered from the current state.
    pub fn query(&self, name: String, arguments_json: String) -> String {
        self.inner.query(&name, &arguments_json)
    }
}

impl Default for MobileChatCore {
    fn default() -> Self {
        Self::new()
    }
}

/// `JSON.stringify([machineId, projectId, sessionId])`, the retained-transcript record key the
/// host passes back as `start`'s `config.retainedKey`.
#[uniffi::export]
pub fn retained_snapshot_key(machine_id: String, project_id: String, session_id: String) -> String {
    ghostex_gx_chat_json::retained_snapshot_key(&machine_id, &project_id, &session_id)
}

/// This binding's version, for diagnostics.
#[uniffi::export]
pub fn core_version() -> String {
    concat!("gx-chat-mobile ", env!("CARGO_PKG_VERSION")).to_string()
}
