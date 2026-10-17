//! When a chat Stop may press Ctrl+C in a Grok Build terminal.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// A Ctrl+C at an idle prompt arms Grok's quit ("Ctrl+c:press again to quit"); the rest absorbs
/// queue and PTY jitter between two writes.
const GROK_INTERRUPT_SPACING: Duration = Duration::from_millis(3_000);

/// The shortcut footer is the last line under the composer box.
const GROK_BUSY_HINT_SCAN_LINES: usize = 4;

/// CDXC:SessionChat 2026-10-17 WHY:
/// Grok Build (1.0.62) ignores Escape during a turn, so chat Stop and `ghostex interrupt-session-chat` answered "interrupted" while the command or answer kept running. Ctrl+C cancels the turn ("Turn cancelled by user"), but at an idle prompt it arms Grok's quit, so Stop presses it only while Grok's footer shows `Ctrl+c:cancel` (painted only during a turn), and at most once every three seconds.
/// SEE-ALSO: server/src/session_chat_send/steps.rs (`session_chat_interrupt_escape_steps`), server/src/session_chat_send/worker.rs (`GuardGrokInterrupt`).
pub(crate) fn grok_turn_is_interruptible(screen: &str) -> bool {
    screen
        .lines()
        .rev()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string())
        .filter(|line| !line.is_empty())
        .take(GROK_BUSY_HINT_SCAN_LINES)
        .any(|line| line.contains("Ctrl+c:cancel"))
}

/// Records a Ctrl+C for this session, or refuses when the previous one is still inside Grok's
/// quit window.
pub(crate) fn claim_grok_interrupt(project_id: &str, session_id: &str) -> bool {
    static LAST_INTERRUPT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let Ok(mut last) = LAST_INTERRUPT.get_or_init(Mutex::default).lock() else {
        return false;
    };
    last.retain(|_, at| at.elapsed() < GROK_INTERRUPT_SPACING);
    let key = crate::server::session_observer_key(project_id, session_id);
    if last.contains_key(&key) {
        return false;
    }
    last.insert(key, Instant::now());
    true
}
