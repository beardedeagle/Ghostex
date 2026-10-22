//! The document's `readAloud` key, which the reply action and the ⋯ menu rows draw from.

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::state::{ChatContext, ChatState};

/// `readAloud` in the document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadAloudDocument {
    /// Draw the controls at all: the feature is on for this computer and this device has a voice.
    pub available: bool,
    /// The ⋯ menu's Auto-read row is checked.
    pub auto_read: bool,
    /// Something of this chat is being read (a reply or a question line): the ⋯ menu shows Stop
    /// reading.
    pub speaking: bool,
    /// The reply being read, whose action reads Stop reading; `null` otherwise.
    pub speaking_id: Option<String>,
    /// Why auto-read is quiet for now: `voiceConversation` or `dictation`; `null` when it is not.
    pub auto_read_held: Option<String>,
}

/// Writes the `readAloud` key.
pub fn document(state: &ChatState, _context: &ChatContext, into: &mut Document) {
    let read_aloud = &state.read_aloud;
    into.read_aloud = ReadAloudDocument {
        available: read_aloud.available,
        auto_read: read_aloud.auto_read,
        speaking: read_aloud.current.is_some(),
        speaking_id: read_aloud
            .current
            .as_ref()
            .and_then(|playing| playing.message_id.clone()),
        auto_read_held: if !read_aloud.auto_read {
            None
        } else if read_aloud.voice_conversation {
            Some("voiceConversation".to_string())
        } else if read_aloud.dictating {
            Some("dictation".to_string())
        } else {
            None
        },
    };
}
