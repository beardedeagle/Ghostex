//! The web build's stand-in for the desktop's `app/voice_agent/`: the names the shared chat files
//! call, with no microphone behind them.
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the page has no echo-cancelled microphone path yet (the contract's `AudioWorklet` capture is not built), so it draws neither the Voice button (its header is its own) nor Dictate (`DICTATION` is macOS only); `native_chat/dictation.rs` still names the speech state it would dictate with.

use gpui::Context;
use gpui_component::speech::SpeechState;

/// A speech state with no recognizer: Dictate is not drawn on the page.
pub(crate) fn grok_dictation_speech_state(cx: &mut Context<SpeechState>) -> SpeechState {
    SpeechState::new(cx).system_fallback(false)
}
