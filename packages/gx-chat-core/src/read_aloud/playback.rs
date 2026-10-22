//! The playback queue: what speaks now, and the one reply (and one question line) waiting behind
//! it.
//!
//! The core owns what is said and when; the host owns the engine. Every effect goes to the state's
//! outbox, which `ChatCore::handle` drains after the turn's publish, so the document that shows
//! "Stop reading" ships with the `speak` that started it.

use crate::effect::Effect;
use crate::read_aloud::attention;
use crate::read_aloud::replies::message_segments;
use crate::read_aloud::speakable::{SegmentKind, SpeechSegment};
use crate::state::{ChatState, Notice, Playing};

/// Reads agent message `message_id` from the start, stopping whatever this chat is saying.
/// Returns false when the message has nothing to say.
pub fn speak_message(state: &mut ChatState, message_id: &str, auto: bool) -> bool {
    let Some(segments) = message_segments(state, message_id).filter(|s| !s.is_empty()) else {
        return false;
    };
    speak(state, Some(message_id.to_string()), segments, auto);
    true
}

/// Says one question or approval line.
pub fn speak_notice(state: &mut ChatState, notice: &Notice) {
    let segments = vec![SpeechSegment {
        text: notice.line.to_string(),
        kind: SegmentKind::Note,
    }];
    speak(state, None, segments, true);
}

fn speak(
    state: &mut ChatState,
    message_id: Option<String>,
    segments: Vec<SpeechSegment>,
    auto: bool,
) {
    stop_current(state);
    let read_aloud = &mut state.read_aloud;
    read_aloud.next_utterance += 1;
    let id = format!("readAloud:{}", read_aloud.next_utterance);
    read_aloud.outbox.push(Effect::Speak {
        id: id.clone(),
        message_id: message_id.clone(),
        segments,
    });
    read_aloud.current = Some(Playing {
        id,
        message_id,
        auto,
    });
}

/// Stops the utterance this chat is speaking, if any. Waiting items stay.
pub fn stop_current(state: &mut ChatState) {
    if let Some(playing) = state.read_aloud.current.take() {
        state
            .read_aloud
            .outbox
            .push(Effect::StopSpeaking { id: playing.id });
    }
}

/// Stops the speech and forgets everything waiting.
pub fn stop_all(state: &mut ChatState) {
    stop_current(state);
    forget_waiting(state);
}

/// Drops what auto-read had lined up, without touching what is speaking.
pub fn forget_waiting(state: &mut ChatState) {
    let read_aloud = &mut state.read_aloud;
    read_aloud.waiting_reply = None;
    read_aloud.waiting_notice = None;
    read_aloud.pending_reply = None;
    read_aloud.pending_notice = None;
}

/// An auto-read reply: now when nothing speaks, else it waits (a newer one replaces it).
///
/// CDXC:VoiceMode 2026-10-20 WHY: a reply that finishes while another is being read waits for it
/// rather than cutting it off, and only the newest waits, so a burst of turns never builds a
/// backlog that keeps talking long after the listener has caught up.
pub fn enqueue_reply(state: &mut ChatState, message_id: String) {
    let read_aloud = &state.read_aloud;
    if read_aloud.current.is_none() && !read_aloud.dictating {
        if !speak_message(state, &message_id, true) {
            play_next(state);
        }
        return;
    }
    state.read_aloud.waiting_reply = Some(message_id);
}

/// A question or approval line: said after anything speaking or waiting.
pub fn enqueue_notice(state: &mut ChatState, notice: Notice) {
    let read_aloud = &state.read_aloud;
    if read_aloud.current.is_none() && read_aloud.waiting_reply.is_none() && !read_aloud.dictating {
        speak_notice(state, &notice);
        return;
    }
    state.read_aloud.waiting_notice = Some(notice);
}

/// Starts the next waiting item once nothing speaks and the microphone is off. A question line
/// whose prompt has been answered since is dropped.
pub fn play_next(state: &mut ChatState) {
    loop {
        let read_aloud = &state.read_aloud;
        if read_aloud.current.is_some() || read_aloud.dictating {
            return;
        }
        if let Some(message_id) = state.read_aloud.waiting_reply.take() {
            if speak_message(state, &message_id, true) {
                return;
            }
            continue;
        }
        if let Some(notice) = state.read_aloud.waiting_notice.take() {
            if attention::current(state).is_some_and(|open| open.key == notice.key) {
                speak_notice(state, &notice);
            }
        }
        return;
    }
}

/// The host finished (or cut off) utterance `id`.
pub fn speech_ended(state: &mut ChatState, id: &str, interrupted: bool) {
    if state
        .read_aloud
        .current
        .as_ref()
        .map(|playing| playing.id.as_str())
        != Some(id)
    {
        // An utterance this chat already stopped or replaced.
        return;
    }
    state.read_aloud.current = None;
    if interrupted {
        // The engine cut it off (another chat took the speaker, a phone call): nothing queued
        // behind it is still wanted.
        forget_waiting(state);
        return;
    }
    play_next(state);
}
