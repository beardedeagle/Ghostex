// The terminal bar's Dictate button: Grok dictation (`voice_agent::grok_dictation_speech_state`)
// turns what the reader says into text, pasted into that session's terminal when they stop.
//
// CDXC:SessionChat 2026-10-08 DECISION:
// User: "show the mic in the terminal view bottom bar to the left of the chat view button". It is
// the chat composer's Dictate control (`native_chat/dictation.rs`) on the terminal bar: press to
// listen, press again to stop; what was heard is pasted into the session's terminal and Enter is
// never pressed. One microphone serves the whole window, so pressing Dictate on another session
// while one listens stops that one first, and its words still go to its own terminal.

use crate::app::model::*;
use crate::*;
use gpui::{AppContext as _, Entity, Subscription, Window};
use gpui_component::speech::{SpeechEvent, SpeechState};

/// Whether this build can dictate: macOS only, like the chat box's Dictate (CDXC:VoiceMode
/// 2026-10-20 DECISION in `native_chat/dictation.rs`). Off everywhere while `DICTATION_ENABLED` is.
pub(super) const TERMINAL_DICTATION: bool =
    crate::app::native_chat::dictation::DICTATION_ENABLED && cfg!(target_os = "macos");

/// The session the microphone is listening for, and the subscription that pastes its result.
pub(crate) struct TerminalDictation {
    session_id: TerminalSessionId,
    speech: Entity<SpeechState>,
    /// From pressing Dictate until the result (or an error) arrives, so the bar can draw its Stop
    /// glyph without reading the speech state during layout.
    listening: bool,
    _subscription: Subscription,
}

impl GhostexGpuiApp {
    /// Whether the microphone is listening (or finishing what it heard) for `session_id`.
    pub(super) fn terminal_dictating(&self, session_id: TerminalSessionId) -> bool {
        self.terminal_dictation
            .as_ref()
            .is_some_and(|dictation| dictation.session_id == session_id && dictation.listening)
    }

    /// Start listening for `session_id`, or stop whatever is listening.
    pub(super) fn toggle_terminal_dictation(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(dictation) = &self.terminal_dictation
            && dictation.listening
        {
            dictation.speech.update(cx, |speech, cx| speech.stop(cx));
            cx.notify();
            return;
        }
        let speech = cx.new(crate::app::voice_agent::grok_dictation_speech_state);
        let subscription = cx.subscribe_in(&speech, window, move |this, _, event, _, cx| {
            let ended = matches!(
                event,
                SpeechEvent::Final(_) | SpeechEvent::Cancelled | SpeechEvent::Error(_)
            );
            match event {
                SpeechEvent::Final(text) => this.paste_terminal_dictation(session_id, text, cx),
                SpeechEvent::Error(error) => {
                    let toast = crate::app::native_chat::dictation::dictation_failure_toast(error);
                    this.receive_gpui_app_toast_bridge_message(&toast, cx);
                }
                _ => {}
            }
            if ended && let Some(dictation) = this.terminal_dictation.as_mut() {
                dictation.listening = false;
            }
            cx.notify();
        });
        speech.update(cx, |speech, cx| speech.start(cx));
        self.terminal_dictation = Some(TerminalDictation {
            session_id,
            speech,
            listening: true,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Paste a finished dictation into the session's terminal, with a trailing space so the next
    /// phrase does not run into it. Enter is never sent.
    fn paste_terminal_dictation(
        &mut self,
        session_id: TerminalSessionId,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        let pasted = format!("{text} ");
        view.update(cx, |view, cx| {
            view.paste_text(&pasted, cx);
        });
    }
}
