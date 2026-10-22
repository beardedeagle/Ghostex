//! The reader's Read aloud gestures, and the composer gestures that stop the reading.

use serde_json::Value;

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::read_aloud::{events, playback};
use crate::state::{ChatContext, ChatState};

/// One Read aloud action from the renderer or a hotkey. Effects go to the outbox, so this returns
/// none.
///
/// CDXC:VoiceMode 2026-10-20 DECISION:
/// User: two ways to start it, on the phone and the desktop together: a per-message Read aloud action (the desktop's reply hover row, the phone's long-press menu) and an Auto-read switch in the composer's ⋯ (More actions) menu. Read aloud on a reply stops whatever is speaking and reads that reply from the start; on the reply already speaking it is Stop reading.
///
/// CDXC:VoiceMode 2026-10-21 SEE-ALSO: the two hotkeys the user asked for (Q31), "Read last reply aloud" (`readAloudLastReply`) and "Stop reading" (`readAloudStop`), are in packages/settings-catalog/src/hotkey_definitions.rs.
pub fn handle(state: &mut ChatState, action: &UserAction, _context: &ChatContext) -> Vec<Effect> {
    match &action.kind {
        ActionKind::ReadAloudMessage => {
            let Some(message_id) = action.param("messageId").and_then(Value::as_str) else {
                return Vec::new();
            };
            if !state.read_aloud.available {
                return Vec::new();
            }
            let speaking_it = state
                .read_aloud
                .current
                .as_ref()
                .and_then(|playing| playing.message_id.as_deref())
                == Some(message_id);
            playback::stop_all(state);
            if !speaking_it {
                playback::speak_message(state, message_id, false);
            }
        }
        ActionKind::ReadAloudLastReply => {
            if !state.read_aloud.available {
                return Vec::new();
            }
            if let Some(message_id) = state.transcript_view.final_ids.last().cloned() {
                playback::stop_all(state);
                playback::speak_message(state, &message_id, false);
            }
        }
        ActionKind::ReadAloudStop => playback::stop_all(state),
        ActionKind::ReadAloudAuto => {
            let on = action
                .param("on")
                .and_then(Value::as_bool)
                .unwrap_or(!state.read_aloud.auto_read);
            events::set_auto(state, on);
            // The host keeps the value for this run and pushes it into every other chat.
            state
                .read_aloud
                .outbox
                .push(Effect::SetReadAloudAuto { on });
        }
        _ => {}
    }
    Vec::new()
}

/// A gesture another family handled that also stops the reading: sending a message in this chat
/// (or answering it) stops the speech, and Auto-read stays on for the next reply.
pub fn after_action(state: &mut ChatState, kind: &ActionKind) {
    if matches!(
        kind,
        ActionKind::Send
            | ActionKind::Queue
            | ActionKind::Compact
            | ActionKind::Answer
            | ActionKind::FormSend
            | ActionKind::AsyncQuestionSend
    ) {
        playback::stop_all(state);
    }
}
