//! The composer's Dictate control: the microphone records to a file first and the recording is
//! transcribed after Stop (`voice_agent::Dictate`, gxserver's `/api/transcribeDictation`); the text
//! is typed into the draft. While it records or transcribes, a small pill just above the
//! microphone shows it with an ✕ that cancels (Escape cancels too); a failed transcription stays in
//! that pill with Transcribe again. Hovering the microphone when a recording has no text yet offers
//! Transcribe previous.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "I want dictating and recording and all of this to show up just as a small pop-up right above the dictating button, basically. And escape cancels shouldn't be shown like this. It should be just an X button that appears there. When I hover, it says escape cancels [...] I imagine it to be like a pill above the dictating button that is centered above it." Then, seeing it in a row of its own above the chat box: "um no the pills are wrong spot please move them right above the button with small gap above it". Every Dictate state (recording with its clock, Transcribing, a failure with Transcribe again) is one capsule centred on the microphone, 6px above it, floating over the bottom of the text field: the user's explicit OK for that overlap (AGENTS.md "Native layout and hit-testing discipline"). The pill is a visible element that takes clicks only on itself; the rest of the text field stays usable. Its ✕'s tooltip says Escape cancels. Supersedes the full-width row with "Esc cancels" and the row-above-the-box spot (both earlier on 2026-10-20). The phone's `DictationBar.tsx` still draws the row until the user confirms the phone part.
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "When I'm transcribing, my caret basically isn't shown anymore unless I click on the chat again. please stop doing this. It should keep my caret shown in the chat so I know where the transcription that I'm doing is going to end up." Pressing the microphone or anything on the pill never takes focus from the chat box (the chat surface tracks focus for any press nothing claims, which blurred the field); the dictated text lands at the caret, which then sits right after it.
//!
//! CDXC:SessionChat 2026-10-08 DECISION: User asked for voice input in the chat box, to the left of the Terminal View button, on the desktop and the phone (GPUI Kit's speech input, #3333, on the desktop). Pressing the microphone starts listening and pressing it again stops; the recognized text is typed into the draft and never sent by itself.
//! CDXC:SessionChat 2026-10-08 SEE-ALSO: `apps/mobile/app/src/chat/native/composer/useDictation.ts` is the phone's control; the toolbar order is `toolbar.rs` `COMPOSER_CONTROLS` and the phone's `menus.ts`.
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO: Escape, the Transcribing ✕, keeping cancelled recordings and Transcribe previous are the user's decision in `apps/desktop/src/app/voice_agent/dictation.rs`.

use super::appearance::ChatAppearance;
use super::cursor::ChatCursor as _;
use super::state::{NativeChatEvent, NativeChatView};
use crate::app::voice_agent::{Dictate, DictateEvent, DictatePhase};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, EntityInputHandler as _,
    FontFeatures, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, canvas, div, px,
};
use gpui_component::speech::{SpeechError, SpeechState};
use serde_json::json;
use std::time::Duration;
use std::{cell::Cell, rc::Rc};

/// The one switch for both Dictate buttons, the chat box's and the terminal bar's.
///
/// CDXC:VoiceMode 2026-10-20 DECISION: User: Dictate uses Grok dictation (xAI speech-to-text through gxserver, with the key kept on this computer) instead of the system speech recognizer. Later the same day the user said "Please make the voice agent feature and the dictating feature and all of those features related to voice mode basically there on Linux and on Windows. They're not Mac only, you know what I mean?" so Dictate is shown on macOS, Windows and Linux, superseding "Windows stays hidden" (and the 2026-10-09 decision that hid both Dictate buttons on every OS because the Windows system recognizer never returned words).
/// CDXC:SessionChat 2026-10-09 SEE-ALSO: the phone's switch is `DICTATION_ENABLED` in `apps/mobile/app/src/chat/native/composer/useDictation.ts`.
pub(crate) const DICTATION_ENABLED: bool = true;

/// Whether this build can dictate: the desktop app on macOS, Windows and Linux, where the app's own
/// audio captures the microphone (`voice_agent/audio.rs`). The web build has no microphone, so the
/// control is not drawn there.
pub(super) const DICTATION: bool = DICTATION_ENABLED
    && cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ));

/// Whether the chat box draws the Dictate button. The web build draws it only under its
/// `?dictatePreview=` switch, which shows the pill's states without a microphone.
pub(super) fn dictation_drawn() -> bool {
    #[cfg(target_family = "wasm")]
    {
        crate::app::voice_agent::dictation_preview()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        DICTATION
    }
}

/// The pill's height, unscaled: a 16px line, 4px above and below, a 1px edge.
const PILL_HEIGHT: f32 = 26.0;
/// Space between the pill's bottom edge and the microphone's top edge, unscaled.
const PILL_GAP: f32 = 6.0;

/// Where the pill sits inside the chat box, from the box's and the microphone's last paint.
struct Placement {
    /// The box's border box (`composer_bounds`, its top may reach up over stacked panels).
    card: Rc<Cell<Bounds<Pixels>>>,
    microphone: Rc<Cell<Bounds<Pixels>>>,
    pill: Rc<Cell<Bounds<Pixels>>>,
    microphone_shown: bool,
    scale: f32,
}

impl Placement {
    /// `(left, bottom)` inside the box's padding box: centred on the microphone and
    /// [`PILL_GAP`] above it, kept inside the box when it is too wide to centre. Flush right
    /// above the toolbar before the first paint, or while the microphone is folded into More
    /// actions.
    fn spot(&self) -> (Pixels, Pixels) {
        let card = self.card.get();
        let microphone = self.microphone.get();
        let width = self.pill.get().size.width;
        // Absolute children are placed in the padding box, one border width inside the card.
        let inner_left = card.left() + px(1.0);
        let inner_bottom = card.bottom() - px(1.0);
        let room = (card.size.width - px(2.0) - width).max(px(0.0));
        let gap = px(PILL_GAP * self.scale);
        if self.microphone_shown && microphone.size.width > px(0.0) && card.size.width > px(0.0) {
            let left = (microphone.center().x - inner_left - width / 2.0).clamp(px(0.0), room);
            let bottom = (inner_bottom - microphone.top() + gap).max(px(0.0));
            (left, bottom)
        } else {
            (room, px(40.0 * self.scale))
        }
    }
}

/// How long the pointer rests on the microphone before Transcribe previous is offered.
const HOVER_DELAY: Duration = Duration::from_millis(350);

/// The app toast for a dictation problem no composer row shows (the microphone refused, nothing
/// waiting for Transcribe previous), from the composer or the terminal bar.
///
/// CDXC:SessionChat 2026-10-09 WHY: User: "pls fix dictate feature it doesnt work at all (at least on windows)". A `sessionChatHostAction` "toast" is dropped by the app, so pressing the microphone did nothing visible. This raw `{type: "toast"}` message reaches the app's toast window from both buttons.
pub(crate) fn dictation_toast(title: &str, message: &str) -> serde_json::Value {
    json!({
        "type": "toast",
        "level": "error",
        "title": title,
        "description": message,
        "durationMs": 12_000,
    })
}

/// Where to allow the microphone when the system refused it (the system recognizer's wording).
const MICROPHONE_ACCESS_HINT: &str = if cfg!(target_os = "windows") {
    "Ghostex can't use the microphone. Turn on Microphone access and Let desktop apps access your microphone in Windows Settings > Privacy & security > Microphone."
} else {
    "Ghostex can't use the microphone. Allow Ghostex under Microphone in System Settings > Privacy & Security."
};

/// What to install when the system cannot recognize its language.
const SPEECH_LANGUAGE_HINT: &str = if cfg!(target_os = "windows") {
    "Windows has no speech recognition for your language. Add a speech language in Windows Settings > Time & language > Speech."
} else {
    "Your Mac can't recognize your language on the device. Check that Dictation is on in System Settings > Keyboard."
};

/// The system speech recognizer's failure toast. Unused with [`system_recognizer_speech_state`].
#[allow(dead_code)]
pub(crate) fn dictation_failure_toast(error: &SpeechError) -> serde_json::Value {
    let description = match error {
        SpeechError::PermissionDenied => MICROPHONE_ACCESS_HINT.to_owned(),
        SpeechError::NoInputDevice => {
            "No microphone was found. Connect one, or choose an input device in your sound settings."
                .to_owned()
        }
        SpeechError::Unsupported => SPEECH_LANGUAGE_HINT.to_owned(),
        SpeechError::Recognizer(error) => format!("{error:#}"),
        SpeechError::Input(error) => format!("The microphone stopped working: {error:#}"),
    };
    dictation_toast("Dictation failed", &description)
}

/// The system speech recognizer's dictation (GPUI Kit's `SystemRecognizer`), which Dictate used
/// before Grok dictation.
///
/// CDXC:VoiceMode 2026-10-20 DECISION: User: "keep but mark the old code as unused". Unused since 2026-10-20: Dictate records and transcribes through `voice_agent::Dictate`. Returning to the system recognizer means driving a `SpeechState` from this again at the two Dictate buttons (here and `render/terminal_agent_action_bar/dictation.rs`).
#[allow(dead_code)]
pub(crate) fn system_recognizer_speech_state(cx: &mut Context<SpeechState>) -> SpeechState {
    SpeechState::new(cx)
}

/// The composer's Dictate state, made the first time the reader uses it.
pub(crate) struct DictationControl {
    dictate: Entity<Dictate>,
    _subscription: Subscription,
    /// The pointer is on the microphone; the hover menu opens after [`HOVER_DELAY`].
    hover: Option<Task<()>>,
    /// The pill at its last paint, and the spot it was last placed at (`Placement`).
    pill: Rc<Cell<Bounds<Pixels>>>,
    placed: Rc<Cell<(Pixels, Pixels)>>,
}

pub(crate) type Dictation = Option<DictationControl>;

impl NativeChatView {
    fn dictate(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Dictate> {
        if let Some(control) = &self.dictation {
            return control.dictate.clone();
        }
        let dictate = cx.new(|_| Dictate::new());
        let subscription =
            cx.subscribe_in(&dictate, window, |this, _, event, window, cx| match event {
                DictateEvent::Text(text) => this.insert_dictation(text, window, cx),
                DictateEvent::Failure { title, message } => {
                    // A failed transcription shows in the pill over the microphone with Transcribe
                    // again; only what the pill does not show becomes a toast.
                    let in_row = this.dictation.as_ref().is_some_and(|control| {
                        matches!(
                            control.dictate.read(cx).phase(),
                            DictatePhase::Failed { .. }
                        )
                    });
                    if !in_row {
                        cx.emit(NativeChatEvent::Host(dictation_toast(title, message)));
                    }
                    cx.notify();
                }
            });
        cx.observe(&dictate, |_, _, cx| cx.notify()).detach();
        self.dictation = Some(DictationControl {
            dictate: dictate.clone(),
            _subscription: subscription,
            hover: None,
            pill: Default::default(),
            placed: Rc::new(Cell::new((px(0.0), px(0.0)))),
        });
        dictate
    }

    /// Whether the microphone is recording (the Stop glyph).
    pub(super) fn dictating(&self, cx: &App) -> bool {
        self.dictation
            .as_ref()
            .is_some_and(|control| control.dictate.read(cx).recording())
    }

    /// Start recording, or stop and transcribe.
    pub(super) fn toggle_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dictate = self.dictate(window, cx);
        dictate.update(cx, |dictate, cx| dictate.toggle(cx));
        cx.notify();
    }

    /// The Dictate hotkey: what pressing the microphone does. Answers false where the chat box
    /// draws no microphone, so the chord keeps its ordinary meaning.
    pub(crate) fn toggle_dictation_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.composer_control_available("dictate") {
            return false;
        }
        self.toggle_dictation(window, cx);
        true
    }

    /// Escape: cancels a recording or a transcription (the recording is kept). Answers whether
    /// there was one.
    pub(super) fn cancel_dictation(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(control) = &self.dictation else {
            return false;
        };
        let dictate = control.dictate.clone();
        let active = {
            let dictate = dictate.read(cx);
            dictate.recording() || dictate.transcribing()
        };
        if active {
            dictate.update(cx, |dictate, cx| dictate.cancel(cx));
        }
        active
    }

    pub(super) fn transcribe_previous_dictation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dictate = self.dictate(window, cx);
        dictate.update(cx, |dictate, cx| dictate.transcribe_previous(cx));
    }

    /// Settings › Voice agent, scrolled to Dictation history.
    pub(super) fn open_dictation_history(&mut self, cx: &mut Context<Self>) {
        cx.emit(NativeChatEvent::Host(json!({
            "type":"open","modal":"settings","initialTab":"voiceAgent","initialSection":"dictations",
        })));
    }

    /// The microphone's menu: Transcribe previous when a recording has no text yet, and the
    /// history. Opened by resting the pointer on the microphone (only when there is something to
    /// transcribe) or by right-clicking it.
    pub(super) fn show_dictation_menu(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut rows = Vec::new();
        if let Some((_, duration_ms, created_at)) = crate::app::voice_agent::previous_recording() {
            let ago = chrono::Utc::now()
                .timestamp_millis()
                .saturating_sub(created_at)
                / 60_000;
            let when = match ago {
                0 => "just now".to_string(),
                1 => "1 min ago".to_string(),
                minutes if minutes < 60 => format!("{minutes} min ago"),
                minutes => format!("{} h ago", minutes / 60),
            };
            rows.push(json!({
                "label": format!(
                    "Transcribe previous ({}, {when})",
                    crate::app::voice_agent::dictation_clock(Duration::from_millis(duration_ms))
                ),
                "command": {"type":"composerHost","action":"transcribePreviousDictation"},
            }));
        }
        rows.push(json!({
            "label": "Dictation history…",
            "command": {"type":"composerHost","action":"dictationHistory"},
        }));
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        let trigger =
            super::toolbar::button_trigger(self.composer_button_bounds.dictate.get(), position);
        self.show_chat_menu(rows, trigger, 260.0, window, cx);
    }

    /// The pointer entered or left the microphone.
    pub(super) fn hover_dictation(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !dictation_drawn() {
            return;
        }
        let dictate = self.dictate(window, cx);
        let idle = matches!(
            dictate.read(cx).phase(),
            DictatePhase::Idle | DictatePhase::Failed { .. }
        );
        if !hovered || !idle {
            if let Some(control) = self.dictation.as_mut() {
                control.hover = None;
            }
            return;
        }
        // The hover asks gxserver what is waiting, so a recording the phone or the terminal bar
        // left is offered too.
        let refresh = cx
            .background_executor()
            .spawn(async { crate::app::voice_agent::refresh_previous() });
        let task = cx.spawn_in(window, async move |this, cx| {
            refresh.await;
            cx.background_executor().timer(HOVER_DELAY).await;
            let _ = this.update_in(cx, |this, window, cx| {
                let still_hovered = this
                    .dictation
                    .as_ref()
                    .is_some_and(|control| control.hover.is_some());
                if still_hovered && crate::app::voice_agent::previous_recording().is_some() {
                    let position = window.mouse_position();
                    this.show_dictation_menu(position, window, cx);
                }
                if let Some(control) = this.dictation.as_mut() {
                    control.hover = None;
                }
            });
        });
        if let Some(control) = self.dictation.as_mut() {
            control.hover = Some(task);
        }
    }

    /// Dictate's pill: a capsule centred over the microphone while it records, transcribes or
    /// failed, floating [`PILL_GAP`] above the button over the bottom of the chat box's text field
    /// (an absolute child of the box, drawn after its content). It takes clicks only on itself.
    pub(super) fn render_dictation_pill(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let control = self.dictation.as_ref()?;
        let phase = control.dictate.read(cx).phase().clone();
        let s = p.scale;
        let failure = match &phase {
            DictatePhase::Idle => return None,
            DictatePhase::Failed { message, .. } => {
                Some(format!("{message} Your recording is kept."))
            }
            _ => None,
        };
        let failed = failure.is_some();
        let label = match &phase {
            DictatePhase::Starting => "Starting…".to_string(),
            DictatePhase::Recording => {
                crate::app::voice_agent::dictation_clock(control.dictate.read(cx).elapsed())
            }
            DictatePhase::Transcribing => "Transcribing".to_string(),
            _ => "Transcription failed".to_string(),
        };
        let recording = matches!(phase, DictatePhase::Recording);
        let close = div()
            .id("dictation-cancel")
            .role(gpui::Role::Button)
            .aria_label(if failed { "Dismiss" } else { "Cancel" })
            .chat_cursor_pointer()
            .flex_shrink_0()
            .size(px(18.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .hover(|style| style.bg(p.foreground.opacity(0.08)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(if failed {
                    "Dismiss (the recording is kept)"
                } else {
                    "Escape cancels"
                })
                .build(window, cx)
            })
            .child(
                gpui::svg()
                    .path("titlebar/x.svg")
                    .size(px(11.0 * s))
                    .text_color(p.card_muted),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(control) = this.dictation.as_ref() {
                    let dictate = control.dictate.clone();
                    dictate.update(cx, |dictate, cx| dictate.cancel(cx));
                }
                cx.notify();
            }));
        let retry = div()
            .id("dictation-retry")
            .role(gpui::Role::Button)
            .aria_label("Transcribe again")
            .chat_cursor_pointer()
            .flex_shrink_0()
            .px(px(8.0 * s))
            .h(px(18.0 * s))
            .flex()
            .items_center()
            .rounded_full()
            .bg(p.foreground.opacity(0.08))
            .hover(|style| style.bg(p.foreground.opacity(0.14)))
            .text_color(p.foreground)
            .child("Transcribe again")
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(control) = this.dictation.as_ref() {
                    let dictate = control.dictate.clone();
                    dictate.update(cx, |dictate, cx| dictate.retry(cx));
                }
                cx.notify();
            }));
        let label = div()
            .id("dictation-label")
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .when(recording, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .size(px(7.0 * s))
                        .rounded_full()
                        .bg(gpui::rgb(0xef4444)),
                )
            })
            .child(
                div()
                    .when(recording, |this| {
                        this.font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    })
                    .text_color(if failed {
                        gpui::rgb(0xef9999).into()
                    } else {
                        p.foreground
                    })
                    .child(label),
            )
            // Why it failed is the label's tooltip, so the pill stays one short line.
            .when_some(failure, |this, failure| {
                this.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(failure.clone()).build(window, cx)
                })
            });
        let microphone_shown = self.dictation_button_shown();
        let placement = Placement {
            card: self.composer_bounds.clone(),
            microphone: self.composer_button_bounds.dictate.clone(),
            pill: control.pill.clone(),
            microphone_shown,
            scale: s,
        };
        let (left, bottom) = placement.spot();
        control.placed.set((left, bottom));
        let placed = control.placed.clone();
        let measured_pill = control.pill.clone();
        let pill = div()
            .id("dictation-pill")
            .role(gpui::Role::Status)
            .absolute()
            .left(left)
            .bottom(bottom)
            .occlude()
            // A press on the pill (its ✕, Transcribe again, or the pill itself) must not move focus
            // to the chat surface, which tracks focus for presses nothing claimed: the chat box
            // keeps its caret where the dictated text will land.
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.prevent_default()
            })
            .h(px(PILL_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .pl(px(if recording { 9.0 } else { 11.0 } * s))
            .pr(px(4.0 * s))
            .border_1()
            .border_color(p.input_border)
            .rounded_full()
            .bg(p.card_panel)
            .text_size(px(12.0 * s))
            .line_height(px(16.0 * s))
            .whitespace_nowrap()
            .child(label)
            .when(failed, |this| this.child(retry))
            .child(close)
            .child(
                // The pill's border box; once the box and the microphone have painted this frame,
                // a spot that moved (a resize, a growing draft, a wider label) is drawn again.
                canvas(
                    move |bounds, window, _| {
                        measured_pill.set(bounds);
                        if placement.spot() != placed.get() {
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset(px(-1.0)),
            );
        Some(pill.into_any_element())
    }

    /// Whether the toolbar draws the microphone this frame (`toolbar.rs` `render_toolbar`'s rule);
    /// otherwise its last painted bounds are stale.
    fn dictation_button_shown(&self) -> bool {
        !self.composer_animation.shown_collapsed()
            && !self.composer_control_overflowed("dictate")
            && self.composer_control_available("dictate")
    }

    /// Type a finished dictation into the draft, a space apart from the text before it.
    fn insert_dictation(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            cx.notify();
            return;
        }
        let spaced = if self.draft.is_empty() || self.draft.ends_with(char::is_whitespace) {
            text.to_owned()
        } else {
            format!(" {text}")
        };
        if let Some(input) = self.input.clone() {
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.replace_text_in_range(None, &spaced, window, cx);
            });
        }
        cx.notify();
    }
}
