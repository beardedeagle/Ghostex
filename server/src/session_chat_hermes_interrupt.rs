//! When a chat Stop may press Ctrl+C in a Hermes Agent terminal.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// Hermes force-quits on a second Ctrl+C within 2 s of the one that interrupted; the rest absorbs
/// queue and PTY jitter between two writes.
const HERMES_INTERRUPT_SPACING: Duration = Duration::from_millis(3_000);

/// The input area sits at the bottom of the screen; the hint is never further up than this.
const HERMES_BUSY_HINT_SCAN_LINES: usize = 12;

/// CDXC:SessionChat 2026-10-17 WHY:
/// Hermes Agent ignores Escape during a turn (a single Escape is only the Alt-key prefix, a double one discards the draft), so chat Stop and `ghostex interrupt-session-chat` answered "interrupted" while the turn kept running. Ctrl+C stops it, but only when the turn is running and the input line is empty: with a draft it clears the draft, at an idle empty prompt it quits Hermes, and a second press within 2 s of the one that interrupted force-quits it. Hermes shows exactly that state as its input placeholder ("msg=interrupt · /queue · /bg · /steer · Ctrl+C cancel"; the command names stay English in every locale), so Stop presses Ctrl+C only while the placeholder shows, and at most once every three seconds.
/// SEE-ALSO: server/src/session_chat_send/steps.rs (`session_chat_interrupt_escape_steps`), server/src/session_chat_send/worker.rs (`GuardHermesInterrupt`).
pub(crate) fn hermes_turn_is_interruptible(screen: &str) -> bool {
    screen
        .lines()
        .rev()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string())
        .filter(|line| !line.is_empty())
        .take(HERMES_BUSY_HINT_SCAN_LINES)
        .any(|line| line.contains("/queue · /bg · /steer · Ctrl+C"))
}

/// Records a Ctrl+C for this session, or refuses when the previous one is still inside Hermes's
/// force-quit window.
pub(crate) fn claim_hermes_interrupt(project_id: &str, session_id: &str) -> bool {
    static LAST_INTERRUPT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let Ok(mut last) = LAST_INTERRUPT.get_or_init(Mutex::default).lock() else {
        return false;
    };
    last.retain(|_, at| at.elapsed() < HERMES_INTERRUPT_SPACING);
    let key = crate::server::session_observer_key(project_id, session_id);
    if last.contains_key(&key) {
        return false;
    }
    last.insert(key, Instant::now());
    true
}
