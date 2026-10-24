//! This computer's gxserver, as the voice sockets reach it.
//!
//! CDXC:VoiceMode 2026-10-23 DECISION: User, asked "On the computer, what about sessions running on another computer you are connected to?", picked "B. Build it now: the call runs on that other computer instead (that computer needs its own voice key)". Dictate and the headset's all-sessions call talk to this computer's gxserver, whose key they use, even from a chat of a remote machine; a call about one session of a connected computer talks to that computer's gxserver instead (`focus.rs`, `voice_agent_endpoint`). Supersedes the 2026-10-20 WHY that every voice socket talks to this computer. The chat views and the terminal bar have no handle on the app, so the app publishes the local endpoint here whenever it learns it (`session_chat_runtime.rs`).

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
