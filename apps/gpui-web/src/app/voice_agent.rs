//! The web build's stand-in for the desktop's `app/voice_agent/`: the names the shared chat files
//! call, with no microphone behind them.
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the page has no echo-cancelled microphone path yet (the contract's `AudioWorklet` capture is not built), so it draws neither the Voice button (its header is its own) nor Dictate (`DICTATION` is desktop only); `native_chat/dictation.rs` still names the Dictate state it would record with.

use std::time::Duration;

use gpui::{Context, EventEmitter};

/// Dictate's phases, as the desktop's `voice_agent::DictatePhase` names them.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) enum DictatePhase {
    Idle,
    Starting,
    Recording,
    Transcribing,
    Failed { kept: (), message: String },
}

#[allow(dead_code)]
pub(crate) enum DictateEvent {
    Text(String),
    Failure { title: String, message: String },
}

/// `?dictatePreview=recording|transcribing|failed` draws the Dictate button on the page, and
/// pressing it shows that state, so the chat box's Dictate pill can be looked at in a browser. No
/// microphone is opened; ✕ (or pressing the button again) goes back to idle.
pub(crate) fn dictation_preview() -> bool {
    preview_phase().is_some()
}

fn preview_phase() -> Option<DictatePhase> {
    let search = web_sys::window().and_then(|window| window.location().search().ok())?;
    let value = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("dictatePreview="))?;
    match value {
        "recording" => Some(DictatePhase::Recording),
        "transcribing" => Some(DictatePhase::Transcribing),
        "failed" => Some(DictatePhase::Failed {
            kept: (),
            message: "xAI didn't accept this request right now (payment required). If you use Grok through your X subscription, its included usage may be used up for the moment; check grok.com/dashboard.".to_string(),
        }),
        _ => None,
    }
}

/// A Dictate that never records; under `?dictatePreview=` it shows the asked-for state.
pub(crate) struct Dictate {
    phase: DictatePhase,
}

impl EventEmitter<DictateEvent> for Dictate {}

#[allow(dead_code)]
impl Dictate {
    pub(crate) fn new() -> Self {
        Self {
            phase: DictatePhase::Idle,
        }
    }

    pub(crate) fn phase(&self) -> &DictatePhase {
        &self.phase
    }

    pub(crate) fn recording(&self) -> bool {
        matches!(self.phase, DictatePhase::Recording)
    }

    pub(crate) fn transcribing(&self) -> bool {
        matches!(self.phase, DictatePhase::Transcribing)
    }

    pub(crate) fn elapsed(&self) -> Duration {
        Duration::from_secs(7)
    }

    pub(crate) fn toggle(&mut self, cx: &mut Context<Self>) {
        self.phase = match self.phase {
            DictatePhase::Idle => preview_phase().unwrap_or(DictatePhase::Idle),
            _ => DictatePhase::Idle,
        };
        cx.notify();
    }

    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.phase = DictatePhase::Idle;
        cx.notify();
    }

    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
        self.phase = DictatePhase::Transcribing;
        cx.notify();
    }

    pub(crate) fn transcribe_previous(&mut self, _cx: &mut Context<Self>) {}
}

pub(crate) fn previous_recording() -> Option<(String, u64, i64)> {
    None
}

pub(crate) fn refresh_previous() {}

pub(crate) fn dictation_clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
