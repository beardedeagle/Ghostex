//! Whether the user is typing in this app right now: the voice agent's follow (gxserver's
//! `focusSession` with `unlessTyping`) leaves the user where they are while they are.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: the voice agent "by default it should switch the user to whichever session it is talking about when the user is not actively typing". Typing means a keystroke into the chat box, a terminal or any other text field within `USER_TYPING_WINDOW`, or an IME composition still open. Command and Fn chords are shortcuts, not typing, and a lone modifier press is not typing either. Keys a CEF page takes straight from AppKit (the Files view, extension pages) are not seen here; their open IME compositions are.
//! SEE-ALSO: server/src/voice_agent/show.rs, apps/desktop/src/app/gx_store/renderer_commands/perform.rs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Once, OnceLock};
use std::time::{Duration, Instant};

use gpui::Keystroke;

/// How long after a keystroke the user still counts as typing.
pub(crate) const USER_TYPING_WINDOW: Duration = Duration::from_secs(4);

static CLOCK: OnceLock<Instant> = OnceLock::new();
/// Milliseconds after `CLOCK` of the last typing keystroke plus one, 0 before the first.
static LAST_TYPED: AtomicU64 = AtomicU64::new(0);

fn millis_since_start(now: Instant) -> u64 {
    let start = *CLOCK.get_or_init(Instant::now);
    now.saturating_duration_since(start).as_millis() as u64
}

/// Watches every keystroke of every window, before any field or binding handles it. Once per
/// process; later calls do nothing.
pub(crate) fn watch_typing(cx: &mut gpui::App) {
    static WATCHING: Once = Once::new();
    WATCHING.call_once(|| {
        cx.intercept_keystrokes(|event, _window, _cx| {
            if is_typing_keystroke(&event.keystroke) {
                let now = millis_since_start(Instant::now());
                LAST_TYPED.store(now + 1, Ordering::Relaxed);
            }
        })
        .detach();
    });
}

fn is_typing_keystroke(keystroke: &Keystroke) -> bool {
    let modifiers = &keystroke.modifiers;
    if modifiers.platform || modifiers.function {
        return false;
    }
    !matches!(
        keystroke.key.as_str(),
        "shift" | "control" | "alt" | "platform" | "function" | "capslock"
    )
}

fn typed_recently(last_typed: u64, now: u64) -> bool {
    last_typed != 0 && now.saturating_sub(last_typed - 1) < USER_TYPING_WINDOW.as_millis() as u64
}

#[cfg(target_os = "macos")]
fn composing_text() -> bool {
    unsafe extern "C" {
        fn GhostexVoiceKeyWindowIsComposingText() -> i32;
    }
    unsafe { GhostexVoiceKeyWindowIsComposingText() != 0 }
}

#[cfg(not(target_os = "macos"))]
fn composing_text() -> bool {
    false
}

/// Whether the user typed within `USER_TYPING_WINDOW` or is composing IME text. Main thread only.
pub(crate) fn user_is_typing() -> bool {
    let now = millis_since_start(Instant::now());
    typed_recently(LAST_TYPED.load(Ordering::Relaxed), now) || composing_text()
}
