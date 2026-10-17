//! CDXC:AgentScreenDetection 2026-10-17 WHY:
//! Grok Build's `ask_user_question` card owns the terminal input while it is open (measured on 1.0.62), so an answer typed into the composer as a message never lands and the send was dropped after reporting `queued`:
//!
//! ```text
//!  ┃  Which color do you prefer?
//!  ┃  1 (○) Red (Recommended)  Prefer red.
//!  ┃  2 (○) Blue               Prefer blue.
//!  ┃  z (○) Type your answer here
//!  ┃  ↑/↓ navigate · y copy                         Enter:submit
//!  Tab:next answer  │  Esc:scrollback  │  Shift+x:dismiss
//! ```
//!
//! From Grok's source (`app/agent_view/interactions.rs`) and measured on 1.0.62: a row's shortcut (1-9, then a-f) picks it and moves to the next question, or submits on the last one; `z` opens the free-text row, whose Enter commits and moves on the same way. A multi-select question ticks each row with Space, then moves on with Right, or submits with Enter on its last question; Enter on an unticked row of a question that is not the last dropped that row from the answer. Letters type into the free-text row when the cursor sits on it, so the cursor moves only with PageUp and Down.
//! SEE-ALSO: the `grok` arm of handle_answer_session_chat_prompt_http in server/src/session_chat_send/answer_http.rs, server/src/session_chat_grok_blocking.rs.

use crate::session_chat::{SessionChatQuestion, SessionChatQuestionSelection};
use crate::session_chat_send::AskAnswerKeyGroup;

const ENTER: &str = "\r";
const SPACE: &str = " ";
const DOWN: &str = "\u{1b}[B";
const PAGE_UP: &str = "\u{1b}[5~";
const NEXT_QUESTION: &str = "\u{1b}[C";
const FREEFORM: &str = "z";

/// Whether Grok's question card is open at `questions`' first question.
pub fn grok_question_card_on_screen(questions: &[SessionChatQuestion], screen_text: &str) -> bool {
    let Some(first) = questions.first() else {
        return false;
    };
    let waiting = crate::session_chat_grok_blocking::detect_grok_blocking_screen(screen_text)
        .is_some_and(|screen| screen.title == "Grok Build is waiting for an answer");
    let needle = crate::session_chat_options::normalize_spaces(first.question.trim());
    waiting
        && !needle.is_empty()
        && screen_text.lines().any(|line| {
            crate::session_chat_options::normalize_spaces(
                &crate::session_chat_options::strip_ansi_sgr(line),
            )
            .contains(&needle)
        })
}

/// A row's own shortcut key, which picks it: `1`-`9`, then `a`-`f` (Grok maps no further letters).
fn row_shortcut(index: usize) -> Option<String> {
    match index {
        0..=8 => Some(((b'1' + index as u8) as char).to_string()),
        9..=14 => Some(((b'a' + (index - 9) as u8) as char).to_string()),
        _ => None,
    }
}

fn cursor_to(row: usize, groups: &mut Vec<AskAnswerKeyGroup>, cursor: &mut usize) {
    groups.extend(std::iter::repeat_n(
        AskAnswerKeyGroup::Raw(DOWN.to_string()),
        row.saturating_sub(*cursor),
    ));
    *cursor = row;
}

/// Keys that answer the card question by question, stopping at the first question with no answer.
pub fn build_grok_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        let selection = selections.get(index);
        let other = selection
            .and_then(|selection| selection.other.as_deref())
            .map(str::trim)
            .unwrap_or_default();
        let mut picked: Vec<usize> = selection
            .map(|selection| selection.indices.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|row| *row < question.options.len())
            .collect();
        picked.sort_unstable();
        picked.dedup();
        if question.multi_select {
            if picked.is_empty() && other.is_empty() {
                break;
            }
            groups.push(AskAnswerKeyGroup::Raw(PAGE_UP.to_string()));
            let mut cursor = 0;
            for row in &picked {
                cursor_to(*row, &mut groups, &mut cursor);
                groups.push(AskAnswerKeyGroup::Raw(SPACE.to_string()));
            }
            if !other.is_empty() {
                groups.push(AskAnswerKeyGroup::Raw(FREEFORM.to_string()));
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
            } else if index + 1 < questions.len() {
                groups.push(AskAnswerKeyGroup::Raw(NEXT_QUESTION.to_string()));
            } else {
                groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
            }
            continue;
        }
        if !other.is_empty() {
            // The free-text row replaces a picked row on a single-select question, so picked
            // labels join the text as one answer.
            let mut answer: Vec<String> = picked
                .iter()
                .filter_map(|row| question.options.get(*row))
                .map(|option| option.label.clone())
                .collect();
            answer.push(other.to_string());
            groups.push(AskAnswerKeyGroup::Raw(FREEFORM.to_string()));
            groups.push(AskAnswerKeyGroup::Text(answer.join(", ")));
            groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
            continue;
        }
        let Some(row) = picked.first().copied() else {
            break;
        };
        match row_shortcut(row) {
            Some(key) => groups.push(AskAnswerKeyGroup::Raw(key)),
            None => {
                groups.push(AskAnswerKeyGroup::Raw(PAGE_UP.to_string()));
                cursor_to(row, &mut groups, &mut 0);
                groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
            }
        }
    }
    groups
}
