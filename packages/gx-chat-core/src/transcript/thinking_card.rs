//! The one line a thought is drawn as. Every reasoning row carries a `thinkingRow` holding its first
//! plain line (`preview`) and whether opening it shows more (`expandable`). Every reasoning row of
//! the turn that is still running also gets a `thinkingCard` holding whether it is the row being
//! written right now (`live`) and its newest finished line (`preview`), so a renderer draws a
//! closed row the reader opens with one click while the agent is still thinking. Once the turn
//! settles the rows lose the mark and fold into the turn's "Previous turn" card as before.
//!
//! CDXC:SessionChat 2026-10-24 DECISION: User, on Codex's thinking in Simple mode: "thinking has to be switched to a simple expandible area, it can't be this big card like it is now for codex. do same for all the agents we support chat for please". A thought is one slim row shaped like a tool row (brain tile, "Thinking…" while it is written or "Thought" after, then its line), closed by default, one click opening the full text below it; no bordered card. The running turn's thought is drawn that way in every mode, and in Simple mode every thought is, so a run of short Codex reasoning summaries reads as quiet lines between the calls instead of bold paragraphs. Supersedes the 2026-10-23 card look (ferpart's #272 request 1); what that decision chose stays: it starts closed while it streams, shows the newest finished line with no word count, the calls a thought made stay in view below it, and there is no setting to hide thinking. Drawn by the desktop's `native_chat/thinking_card.rs` and the phone's `ThinkingLine` in `MessageRow.tsx`.

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

    /// Adds `thinkingCard` to a projected reasoning row of the running turn. Any other row keeps
    /// the model it shares with the projection cache.
    pub fn mark(&self, message: &ChatMessage, row: &mut std::sync::Arc<Value>) {
        if !self.ids.iter().any(|id| id == &message.id) || !row.is_object() {
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
        if let Value::Object(fields) = std::sync::Arc::make_mut(row) {
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

/// The longest one-line thought that stays closed: a short headline fits the row on a phone, a
/// longer line is cut with an ellipsis and has to open to be read.
const CLOSED_LINE_CHARS: usize = 48;

/// A thought's one line: its first plain line, and whether its full text says more than that.
///
/// CDXC:SessionChat 2026-10-25 WHY: Renderers draw the line truncated to one row, so a one-line thought longer than a headline (OpenCode writes a whole sentence of reasoning on one line) must open too, or its end is never readable.
pub fn thinking_row(body: &str) -> Value {
    let preview = crate::transcript::message_text::plain_reasoning_teaser(body);
    let plain = crate::transcript::message_text::plain_reasoning_text(body);
    let lines = plain
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .count();
    json!({
        "expandable": lines > 1
            || body.contains("```")
            || preview.chars().count() > CLOSED_LINE_CHARS,
        "preview": preview,
    })
}

#[cfg(test)]
mod tests {
    use super::thinking_row;
    use serde_json::json;

    #[test]
    fn codex_summary_is_one_closed_line() {
        // Codex 0.163 records only a reasoning summary's bold headline.
        assert_eq!(
            thinking_row("**Reading the main instructions**"),
            json!({"preview": "Reading the main instructions", "expandable": false})
        );
    }

    #[test]
    fn claude_thinking_opens_to_its_full_text() {
        let thought = "The user wants me to fix average().\n\nLikely cause: it adds 1 after dividing, and an empty list divides by zero.";
        assert_eq!(
            thinking_row(thought),
            json!({"preview": "The user wants me to fix average().", "expandable": true})
        );
    }

    #[test]
    fn grok_thinking_with_a_heading() {
        let thought = "**Planning the fix**\n\nRead the files first, then run the tests.";
        assert_eq!(thinking_row(thought)["preview"], "Planning the fix");
        assert_eq!(thinking_row(thought)["expandable"], true);
    }
}
