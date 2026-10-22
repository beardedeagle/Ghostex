//! Read aloud: the agent's replies spoken by this device's own voice.
//!
//! The rules live here so the desktop, the web build and the phone speak the same words at the
//! same moments; each app only owns its speech engine. The host tells the chat what it knows
//! ([`ReadAloudEvent`]: the feature is on, Auto-read is on, this chat is on screen, a voice agent
//! conversation is open, the microphone is recording, an utterance ended), the renderer sends the
//! reader's gestures as actions (`readAloudMessage`, `readAloudLastReply`, `readAloudStop`,
//! `readAloudAuto`), and the core answers with [`crate::Effect::Speak`],
//! [`crate::Effect::StopSpeaking`] and [`crate::Effect::SetReadAloudAuto`], plus the document's
//! `readAloud` key the controls draw from. Client contract:
//! `docs/2026-10-20/voice-mode/read-aloud-client-contract.md`.
//!
//! - `speakable.rs` turns a reply's Markdown into the segments an engine speaks.
//! - `replies.rs` finds a reply's Markdown by message id.
//! - `auto_read.rs` decides which reply auto-read speaks and when.
//! - `attention.rs` is the line said when a turn ends on a question or a permission prompt.
//! - `playback.rs` is the one-reply-waiting queue.
//! - `events.rs` and `actions.rs` take the host's inputs and the reader's gestures.
//! - `document.rs` writes the `readAloud` key.
//!
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO: the built-in switch (`readAloud`, `readAloudHidden`), the Voice and Speed rows and the two hotkeys are in packages/settings-catalog; gxserver lists the computer's switched-on features in the phone's inventory (`builtInFeatures`, server/src/ghostex_cli/sessions/mobile_summary.rs).

mod actions;
mod attention;
mod auto_read;
mod document;
mod events;
mod playback;
mod replies;
pub mod speakable;

pub use crate::read_aloud::actions::{after_action, handle};
pub use crate::read_aloud::auto_read::{observe, SETTLE_MS, SETTLE_TIMER};
pub use crate::read_aloud::document::{document, ReadAloudDocument};
pub use crate::read_aloud::events::{handle_event, ReadAloudEvent};
pub use crate::read_aloud::replies::{message_segments, reply_markdown};
pub use crate::read_aloud::speakable::{speakable_segments, SegmentKind, SpeechSegment};
