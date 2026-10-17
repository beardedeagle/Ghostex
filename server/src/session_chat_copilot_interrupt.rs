//! When a chat Stop may press Ctrl+C in a Copilot CLI terminal.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// A second Ctrl+C soon after the first is Copilot's exit chord; the rest absorbs queue and PTY
/// jitter between two writes.
const COPILOT_INTERRUPT_SPACING: Duration = Duration::from_millis(3_000);

/// The working line sits just under the composer box at the bottom of the screen.
const COPILOT_BUSY_HINT_SCAN_LINES: usize = 6;

/// CDXC:SessionChat 2026-10-17 WHY:
/// Copilot CLI (1.0.98) reads chat Stop's Escape as "move the running shell command to the background": the turn kept running and later printed its answer anyway. Ctrl+C aborts the turn ("Operation cancelled by user", an `abort` event in its transcript), but at an idle prompt it arms Copilot's exit, so Stop presses it only while Copilot's working line ("Working · … esc interrupt") shows, and at most once every three seconds.
/// SEE-ALSO: server/src/session_chat_send/steps.rs (`session_chat_interrupt_escape_steps`), server/src/session_chat_send/worker.rs (`GuardCopilotInterrupt`).
pub(crate) fn copilot_turn_is_interruptible(screen: &str) -> bool {
    screen
        .lines()
        .rev()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string())
        .filter(|line| !line.is_empty())
        .take(COPILOT_BUSY_HINT_SCAN_LINES)
        .any(|line| line.contains("esc interrupt"))
}

/// Records a Ctrl+C for this session, or refuses when the previous one is still inside Copilot's
/// exit window.
pub(crate) fn claim_copilot_interrupt(project_id: &str, session_id: &str) -> bool {
    static LAST_INTERRUPT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let Ok(mut last) = LAST_INTERRUPT.get_or_init(Mutex::default).lock() else {
        return false;
    };
    last.retain(|_, at| at.elapsed() < COPILOT_INTERRUPT_SPACING);
    let key = crate::server::session_observer_key(project_id, session_id);
    if last.contains_key(&key) {
        return false;
    }
    last.insert(key, Instant::now());
    true
}
