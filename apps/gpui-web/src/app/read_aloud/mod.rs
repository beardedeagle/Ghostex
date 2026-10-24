//! The desktop's `app/read_aloud/` for a browser page: the page's `speechSynthesis` is the one
//! speaker. The chat host (`gx_chat/read_aloud.rs`, shared) performs the core's `speak` and
//! `stopSpeaking` through it and hears the end back as `speechEnded`; the controls are the
//! desktop's own chat files.
//!
//! CDXC:VoiceMode 2026-10-22 WHY: a browser speaks only after the page has had a user gesture, so turning Auto-read on primes the speaker with an empty utterance from inside that click ([`prime`]); pressing Read aloud is a gesture itself. Chrome stops an utterance after about 15 seconds of speech, which the core's segments of at most 1,000 characters stay under.

use std::cell::RefCell;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{SpeechSynthesis, SpeechSynthesisUtterance, SpeechSynthesisVoice};

/// The utterances of the token speaking now, with the handlers that report its end.
struct Speaking {
    token: u64,
    utterances: Vec<SpeechSynthesisUtterance>,
    _handlers: Vec<Closure<dyn FnMut()>>,
}

thread_local! {
    static SPEAKING: RefCell<Option<Speaking>> = const { RefCell::new(None) };
    /// The built-in features on for the computer serving the page (`/api/webBootstrap`).
    static COMPUTER_FEATURES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn synthesis() -> Option<SpeechSynthesis> {
    web_sys::window()?.speech_synthesis().ok()
}

/// What the page's handshake said is on for the computer serving it (`builtInFeatures`).
pub(crate) fn set_computer_features(features: Vec<String>) {
    COMPUTER_FEATURES.with(|current| *current.borrow_mut() = features);
}

/// Whether built-in feature `id` is on for the computer serving the page (`builtInFeatures`).
pub(crate) fn computer_feature_on(id: &str) -> bool {
    COMPUTER_FEATURES.with(|features| features.borrow().iter().any(|known| known == id))
}

/// Read aloud is on for the computer serving the page and this browser has a speaker
/// (`window.speechSynthesis`); contract section 2.
pub(crate) fn available() -> bool {
    let on = COMPUTER_FEATURES.with(|features| {
        features
            .borrow()
            .iter()
            .any(|id| id == ghostex_settings_catalog::built_in_extensions::READ_ALOUD)
    });
    on && synthesis().is_some()
}

/// Speaks `texts` in order under `token`, replacing anything speaking, with the computer's
/// Voice when this browser has it and its Speed.
pub(crate) fn speak(token: u64, texts: Vec<String>, voice: &str, speed: f64) {
    let Some(synthesis) = synthesis() else {
        crate::app::gx_chat::read_aloud_speech_ended(token, true);
        return;
    };
    // The handlers of what was speaking go first, so its cancel is not reported.
    silence_current();
    synthesis.cancel();
    let chosen = pick_voice(&synthesis, voice);
    let last = texts.len().saturating_sub(1);
    let mut utterances = Vec::new();
    let mut handlers: Vec<Closure<dyn FnMut()>> = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        let Ok(utterance) = SpeechSynthesisUtterance::new_with_text(text) else {
            continue;
        };
        if let Some(chosen) = &chosen {
            utterance.set_voice(Some(chosen));
            utterance.set_lang(&chosen.lang());
        }
        utterance.set_rate(speed.clamp(0.1, 10.0) as f32);
        if index == last {
            let ended = Closure::<dyn FnMut()>::new(move || report(token, false));
            utterance.set_onend(Some(ended.as_ref().unchecked_ref()));
            handlers.push(ended);
        }
        let failed = Closure::<dyn FnMut()>::new(move || report(token, true));
        utterance.set_onerror(Some(failed.as_ref().unchecked_ref()));
        handlers.push(failed);
        utterances.push(utterance);
    }
    if utterances.is_empty() {
        crate::app::gx_chat::read_aloud_speech_ended(token, false);
        return;
    }
    // Kept before speaking: a browser that refuses (no user gesture yet) fires `error` inside
    // `speak`, and that end must still be heard.
    let queued = utterances.clone();
    SPEAKING.with(|speaking| {
        *speaking.borrow_mut() = Some(Speaking {
            token,
            utterances,
            _handlers: handlers,
        });
    });
    for utterance in &queued {
        synthesis.speak(utterance);
    }
}

/// Stops whatever is speaking. The end is not reported: the chat already forgot the utterance.
pub(crate) fn stop() {
    silence_current();
    if let Some(synthesis) = synthesis() {
        synthesis.cancel();
    }
}

/// Speaks nothing, from inside the reader's click, so the browser lets later replies play.
pub(crate) fn prime() {
    if let (Some(synthesis), Ok(utterance)) =
        (synthesis(), SpeechSynthesisUtterance::new_with_text(""))
    {
        synthesis.speak(&utterance);
    }
}

/// Unhooks and forgets the utterances speaking now.
fn silence_current() {
    let previous = SPEAKING.with(|speaking| speaking.borrow_mut().take());
    if let Some(previous) = previous {
        for utterance in &previous.utterances {
            utterance.set_onend(None);
            utterance.set_onerror(None);
        }
    }
}

/// The end of `token`, reported once, from a later task: the host may start the next reply at
/// once, which replaces the handler this is called from.
fn report(token: u64, interrupted: bool) {
    let current = SPEAKING.with(|speaking| {
        speaking
            .borrow()
            .as_ref()
            .is_some_and(|speaking| speaking.token == token)
    });
    if !current {
        return;
    }
    // Unhooked now so a later `error` of the same utterances says nothing more.
    SPEAKING.with(|speaking| {
        if let Some(speaking) = speaking.borrow().as_ref() {
            for utterance in &speaking.utterances {
                utterance.set_onend(None);
                utterance.set_onerror(None);
            }
        }
    });
    let Some(window) = web_sys::window() else {
        return;
    };
    let later = Closure::once_into_js(move || {
        crate::app::gx_chat::read_aloud_speech_ended(token, interrupted);
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(later.unchecked_ref(), 0);
}

/// The browser voice for the computer's Voice setting, or none for the browser's default.
///
/// CDXC:VoiceMode 2026-10-22 WHY: the setting holds the desktop's voice id (on a Mac `com.apple.voice.compact.en-US.Samantha`), and a browser names the same voice differently: Safari's `voiceURI` is that id, Chrome's is the bare name ("Samantha"). So the page matches the id, then the name, then the id's last part in the id's language, then that part anywhere; a voice this browser lacks reads with its default (contract section 6).
fn pick_voice(synthesis: &SpeechSynthesis, wanted: &str) -> Option<SpeechSynthesisVoice> {
    if wanted.is_empty() {
        return None;
    }
    let voices: Vec<SpeechSynthesisVoice> = synthesis
        .get_voices()
        .iter()
        .filter_map(|voice| voice.dyn_into::<SpeechSynthesisVoice>().ok())
        .collect();
    let tail = wanted.rsplit('.').next().unwrap_or(wanted);
    let language = wanted
        .split('.')
        .find(|part| part.len() <= 7 && part.contains('-'));
    voices
        .iter()
        .find(|voice| voice.voice_uri() == wanted)
        .or_else(|| voices.iter().find(|voice| voice.name() == wanted))
        .or_else(|| {
            voices.iter().find(|voice| {
                voice.name() == tail && language.is_none_or(|language| voice.lang() == language)
            })
        })
        .or_else(|| voices.iter().find(|voice| voice.name() == tail))
        .cloned()
}
