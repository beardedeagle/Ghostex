//! Spoken announcements: while any voice conversation is open, every session's state is read every
//! two seconds and a session that finishes its turn or starts waiting on a question or an approval
//! is announced to every open conversation.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: while a voice conversation is open it speaks up on its own with short announcements when a session finishes its turn or starts waiting on a question or an approval ("Sidebar fix finished").
//! CDXC:VoiceMode 2026-10-20 WHY:
//! Polling the session rows (the coordinator supervisor's approach, `classify_thread_session`) instead of hooking the activity endpoints: a question card can arrive through a hook that changes no activity. A finished turn is the Stop hook's new turn-complete attention event, so a turn shorter than the poll is still announced (live test: a one-word reply finished between two reads and was missed); agents without hooks fall back to a working state followed by an idle one, each held for two reads so a turn that flickers idle between tool calls is not called finished.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::conversation::{Announcement, Input};
use super::registry;
use super::sessions::{self, VoiceState};
use crate::server::AppState;

const POLL: Duration = Duration::from_secs(2);

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Starts the watcher if it is not running; it stops by itself once no conversation is open.
pub(super) fn ensure_running(state: &Arc<AppState>) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let mut stable: HashMap<(String, String), VoiceState> = HashMap::new();
        let mut seen: HashMap<(String, String), VoiceState> = HashMap::new();
        let mut turn_events: HashMap<(String, String), Option<String>> = HashMap::new();
        let mut seeded = false;
        loop {
            if registry::count() == 0 {
                RUNNING.store(false, Ordering::SeqCst);
                // A conversation opened between the check and the store restarts the watcher.
                if registry::count() == 0 || RUNNING.swap(true, Ordering::SeqCst) {
                    return;
                }
            }
            if let Ok(rows) = sessions::list_sessions_async(&state).await {
                let mut lines: Vec<Announcement> = Vec::new();
                let mut present: HashSet<(String, String)> = HashSet::new();
                for row in &rows {
                    let key = (row.project_id.clone(), row.session_id.clone());
                    present.insert(key.clone());
                    let known = turn_events.contains_key(&key);
                    let previous_event = turn_events
                        .insert(key.clone(), row.turn_event.clone())
                        .flatten();
                    let new_turn = seeded
                        && known
                        && row.turn_event.is_some()
                        && row.turn_event != previous_event
                        && row.state == VoiceState::Finished;
                    if new_turn {
                        stable.insert(key.clone(), row.state);
                        seen.insert(key.clone(), row.state);
                        lines.push(Announcement {
                            text: format!("\"{}\" finished its turn.", row.title),
                            kind: "turnFinished",
                            project_id: row.project_id.clone(),
                            session_id: row.session_id.clone(),
                        });
                        continue;
                    }
                    let previous_seen = seen.insert(key.clone(), row.state);
                    if previous_seen != Some(row.state) {
                        continue;
                    }
                    let before = stable.insert(key.clone(), row.state);
                    if !seeded || before == Some(row.state) {
                        continue;
                    }
                    // A session first seen now (just opened, or just started by the voice agent) is
                    // announced from its next change on.
                    let was_busy = matches!(before, Some(VoiceState::Working));
                    let was_awake = !matches!(before, None | Some(VoiceState::Sleeping));
                    let (text, kind) = match row.state {
                        VoiceState::Finished if was_busy && row.turn_event.is_none() => (
                            format!("\"{}\" finished its turn.", row.title),
                            "turnFinished",
                        ),
                        VoiceState::WaitingOnQuestion if was_awake => (
                            format!("\"{}\" is waiting on a question.", row.title),
                            "waitingOnQuestion",
                        ),
                        VoiceState::WaitingOnApproval if was_awake => (
                            format!("\"{}\" is waiting for your approval.", row.title),
                            "waitingOnApproval",
                        ),
                        _ => continue,
                    };
                    lines.push(Announcement {
                        text,
                        kind,
                        project_id: row.project_id.clone(),
                        session_id: row.session_id.clone(),
                    });
                }
                stable.retain(|key, _| present.contains(key));
                turn_events.retain(|key, _| present.contains(key));
                seen.retain(|key, _| present.contains(key));
                seeded = true;
                if !lines.is_empty() {
                    for tx in registry::senders() {
                        let _ = tx.send(Input::Announce(lines.clone()));
                    }
                }
            }
            tokio::time::sleep(POLL).await;
        }
    });
}
