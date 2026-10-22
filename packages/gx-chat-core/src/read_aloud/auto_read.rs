//! Which reply auto-read speaks, and when.
//!
//! [`observe`] runs once per turn, right after the transcript projection has recomputed `finalIds`
//! (`ChatCore::republish`), because "the agent's final message of the turn" is exactly an id that
//! enters `finalIds`: `final_assistant_message_ids` already skips the commentary between tool
//! calls, ends no turn on a harness row or a held prompt, gives an interrupted turn no final reply,
//! and gives a slash command its own turn.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: only agent messages are read (never your messages, tool calls or thinking), and auto-read speaks only the agent's final message of each turn, not the commentary between tool calls. Auto-read speaks only the chat on screen, never other sessions; the host says which chat that is (`setOnScreen`), and a subagent's transcript is never read.

use crate::read_aloud::{attention, playback};
use crate::state::{ChatContext, ChatState};

/// The settle timer's key in the core's timer table.
pub const SETTLE_TIMER: &str = "readAloud.settle";

/// How long a new final reply (or question) must stay before auto-read speaks it.
///
/// CDXC:VoiceMode 2026-10-21 WHY: the working signal can drop for a moment between two tool calls,
/// and for that moment the newest commentary looks final; speaking it would read commentary aloud
/// and then the reply again. A reply still final 1.5 seconds later is the turn's real end, and the
/// delay is too short to notice in speech.
pub const SETTLE_MS: f64 = 1500.0;

/// Looks at the transcript after the projection settled: takes the baseline, notices new final
/// replies and questions, and speaks the ones that have settled.
pub fn observe(state: &mut ChatState, context: &ChatContext) {
    let armed = {
        let read_aloud = &state.read_aloud;
        read_aloud.available
            && read_aloud.auto_read
            && read_aloud.on_screen
            && !read_aloud.voice_conversation
    };
    if !armed {
        if state.read_aloud.armed {
            disarm(state);
        }
        return;
    }
    if !state.read_aloud.armed {
        state.read_aloud.armed = true;
        state.read_aloud.primed = false;
    }
    let final_ids = state.transcript_view.final_ids.clone();
    let open = attention::current(state);
    let live = state.messages.position.frame_arrived;
    let read_aloud = &mut state.read_aloud;

    /*
    CDXC:VoiceMode 2026-10-20 DECISION:
    User: turning Auto-read on reads nothing already on screen; the next final reply is read. The same holds when a chat comes on screen with Auto-read on, and when a voice agent conversation hangs up. The baseline waits for the live transcript (the socket's first snapshot), so the replies a cached transcript was missing are not taken for new ones.
    */
    if !read_aloud.primed {
        read_aloud.seen = final_ids.iter().cloned().collect();
        read_aloud.announced = open.map(|notice| notice.key);
        read_aloud.primed = live;
        return;
    }

    if final_ids.is_empty() {
        // A cleared conversation: its next reply is new whatever it was called before.
        read_aloud.seen.clear();
    } else if !read_aloud.seen.is_empty()
        && !final_ids.iter().any(|id| read_aloud.seen.contains(id))
    {
        // Every reply auto-read knew is gone at once: the transcript was replaced under it (a
        // different conversation), so it starts over from this one without reading it.
        read_aloud.seen = final_ids.iter().cloned().collect();
        return;
    }

    /*
    CDXC:VoiceMode 2026-10-21 DECISION:
    User (Q29): after a reconnect, read only the newest reply that finished while you weren't listening. Of the final replies that arrived at once, only the newest is read and the older ones are dropped. A reply is new only when it comes after the newest one already counted, so Load earlier never reads an old reply.
    */
    let newest_seen = final_ids
        .iter()
        .rposition(|id| read_aloud.seen.contains(id));
    let fresh = final_ids
        .iter()
        .enumerate()
        .rev()
        .find(|(at, id)| !read_aloud.seen.contains(*id) && newest_seen.is_none_or(|n| *at > n))
        .map(|(_, id)| id.clone());
    read_aloud.seen.extend(final_ids.iter().cloned());
    if let Some(id) = fresh {
        read_aloud.pending_reply = Some(id);
    }
    if let Some(notice) = open {
        if read_aloud.announced.as_deref() != Some(notice.key.as_str()) {
            read_aloud.announced = Some(notice.key.clone());
            read_aloud.pending_notice = Some(notice);
        }
    }

    if state.core.timer_fired(SETTLE_TIMER) {
        promote(state);
    }
    let waiting =
        state.read_aloud.pending_reply.is_some() || state.read_aloud.pending_notice.is_some();
    if waiting {
        state
            .core
            .timers
            .arm_once(SETTLE_TIMER, context.now_ms, SETTLE_MS);
    }
}

/// Speaks (or lines up) what has stayed new for the settle delay.
fn promote(state: &mut ChatState) {
    if let Some(message_id) = state.read_aloud.pending_reply.take() {
        if state.transcript_view.final_ids.contains(&message_id) {
            playback::enqueue_reply(state, message_id);
        }
    }
    if let Some(notice) = state.read_aloud.pending_notice.take() {
        if attention::current(state).is_some_and(|open| open.key == notice.key) {
            playback::enqueue_notice(state, notice);
        }
    }
}

/// Auto-read stopped listening: forget what it lined up, so the next time it listens it starts from
/// a new baseline.
fn disarm(state: &mut ChatState) {
    playback::forget_waiting(state);
    state.core.timers.cancel(SETTLE_TIMER);
    let read_aloud = &mut state.read_aloud;
    read_aloud.armed = false;
    read_aloud.primed = false;
}
