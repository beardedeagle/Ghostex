//! This computer's gxserver, as the voice sockets reach it.
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the API keys live in this computer's gxserver, so the voice conversation and dictation always talk to it, even from a chat of a remote machine. The chat views and the terminal bar have no handle on the app, so the app publishes the local endpoint here whenever it learns it (`session_chat_runtime.rs`).

use std::sync::Mutex;

use ghostex_gx_chat_client::Endpoint;

static LOCAL_ENDPOINT: Mutex<Option<Endpoint>> = Mutex::new(None);

/// Records where this computer's gxserver is. An empty value forgets it.
pub(crate) fn set_local_voice_endpoint(base_url: &str, auth_token: &str) {
    let endpoint = Endpoint::new(base_url, auth_token);
    if let Ok(mut slot) = LOCAL_ENDPOINT.lock() {
        *slot = endpoint.usable().then_some(endpoint);
    }
}

/// This computer's gxserver, once the app knows it.
pub(crate) fn local_voice_endpoint() -> Option<Endpoint> {
    LOCAL_ENDPOINT.lock().ok().and_then(|slot| slot.clone())
}
