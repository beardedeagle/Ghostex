//! Read aloud's state: what the host told the chat, what auto-read has already heard, and what is
//! speaking or waiting.
//!
//! **This file belongs to `crate::read_aloud`.** No family edits it; families read nothing here.

use std::collections::BTreeSet;

use crate::effect::Effect;

/// Everything Read aloud remembers for one chat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReadAloudState {
    // ---- what the host says ------------------------------------------------
    /// The built-in switch is on for this chat's computer and this device has a voice.
    pub available: bool,
    /// Auto-read is on for this run of the app.
    pub auto_read: bool,
    /// This chat is the one on screen.
    pub on_screen: bool,
    /// A voice agent conversation is open.
    pub voice_conversation: bool,
    /// The microphone is recording (Dictate).
    pub dictating: bool,

    // ---- what auto-read has heard ------------------------------------------
    /// Auto-read was listening at the last look.
    pub armed: bool,
    /// The baseline is taken from a live transcript, so a final reply that is not in `seen` is new.
    pub primed: bool,
    /// Every final reply id auto-read has already counted, read or not.
    pub seen: BTreeSet<String>,
    /// The question or approval already announced (or already on screen when auto-read started).
    pub announced: Option<String>,
    /// The newest new final reply, waiting out the settle delay before it is read.
    pub pending_reply: Option<String>,
    /// The new question or approval, waiting out the same delay.
    pub pending_notice: Option<Notice>,

    // ---- what is speaking ----------------------------------------------------
    /// The utterance the host is speaking for this chat.
    pub current: Option<Playing>,
    /// The final reply auto-read reads when the current one finishes; a newer one replaces it.
    pub waiting_reply: Option<String>,
    /// The question or approval line said after the waiting reply.
    pub waiting_notice: Option<Notice>,
    /// The counter utterance ids are drawn from.
    pub next_utterance: u64,
    /// The effects this turn raised, drained by [`crate::ChatCore::handle`].
    pub outbox: Vec<Effect>,
}

/// One utterance the host is speaking.
#[derive(Clone, Debug, PartialEq)]
pub struct Playing {
    /// The id the host reports back in `speechEnded`.
    pub id: String,
    /// The reply being read, or `None` for a question or approval line.
    pub message_id: Option<String>,
    /// Auto-read started it, rather than the reader's Read aloud.
    pub auto: bool,
}

/// A question or approval line, keyed by the prompt it announces.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub key: String,
    pub line: &'static str,
}
