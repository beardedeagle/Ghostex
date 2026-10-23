//! The running turn's thinking as a one-line card from its first word (GitHub #272). Every
//! reasoning row of the turn that is still running gets a `thinkingCard` holding whether it is the
//! row being written right now (`live`) and its newest finished line (`preview`), so a renderer
//! draws a closed card the reader opens with one click while the agent is still thinking. Once the
//! turn settles the rows lose the mark and fold into the turn's "Previous turn" card as before.
//!
//! CDXC:SessionChat 2026-10-23 DECISION: User (answering ferpart's #272 request 1): thinking becomes a card while it streams, by default, for every agent, with no setting. The card starts closed while it streams and shows "Thinking…" with the newest finished line ("newest finished line is good with me"), no word count. The tool calls a thought made stay in view below the card. No setting to hide thinking entirely ("not now"). Drawn by the desktop's `native_chat/thinking_card.rs` and the phone's `ThinkingCard` in `MessageRow.tsx`.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{json, Value};

use crate::transcript::turns::opens_turn;

/// The reasoning rows under the last prompt while the agent works, and the id of the one that
/// is the newest row of the transcript (the thought being written).
pub struct RunningThinking {
    ids: Vec<String>,
    live: Option<String>,
}

impl RunningThinking {
    pub fn new(messages: &[ChatMessage], working: bool) -> Self {
        if !working {
            return Self {
                ids: Vec::new(),
                live: None,
            };
        }
        let start = messages.iter().rposition(opens_turn).map_or(0, |at| at + 1);
        let tail = &messages[start.min(messages.len())..];
        let ids = tail
            .iter()
            .filter(|message| message.role == ChatRole::Reasoning)
            .map(|message| message.id.clone())
            .collect();
        let live = tail
            .last()
            .filter(|message| message.role == ChatRole::Reasoning)
            .map(|message| message.id.clone());
        Self { ids, live }
    }

    /// The reasoning rows that get a card.
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    /// The card a reasoning row gets: `Some(live)`, or `None` when it gets none.
    pub fn card(&self, id: &str) -> Option<bool> {
        self.ids
            .iter()
            .any(|known| known == id)
            .then(|| self.live.as_deref() == Some(id))
    }

    /// Adds `thinkingCard` to a projected reasoning row of the running turn.
    pub fn mark(&self, message: &ChatMessage, row: &mut Value) {
        if !self.ids.iter().any(|id| id == &message.id) {
            return;
        }
        let text: String = message
            .blocks
            .iter()
            .filter_map(|block| match block {
                ChatBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if let Value::Object(fields) = row {
            fields.insert(
                "thinkingCard".to_string(),
                json!({
                    "live": self.live.as_deref() == Some(message.id.as_str()),
                    "preview": preview(&text),
                }),
            );
        }
    }
}

/// The newest finished line of a thought, without its Markdown marks. The line still being
/// written is cut mid-word, so it only counts once its newline arrives.
fn preview(text: &str) -> String {
    let finished = if text.ends_with('\n') {
        text
    } else {
        text.rsplit_once('\n').map_or(text, |(done, _)| done)
    };
    finished
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '*', '-', '>', ' '])
                .trim()
        })
        .rfind(|line| !line.is_empty() && !line.starts_with("```"))
        .unwrap_or_default()
        .replace("**", "")
        .replace('`', "")
}
