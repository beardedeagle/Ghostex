//! CDXC:AgentScreenDetection 2026-10-18 WHY:
//! ZCode's `AskUserQuestion` opens a choice dialog that replaces its input box while it waits, so an answer typed into the input box as a message never landed: the send waited for an input box that was not on screen and failed ("The zcode input box is not on screen yet"), and the picker kept waiting (GitHub #251). The dialog is answered with its own keys instead:
//!
//! ```text
//! Color · 1/2
//! Which color do I prefer?
//! > Red
//!   Green
//!   Other...
//! Type to filter · Up/Down choose · Ctrl+O details · ←/→ or PgUp/PgDn scroll · Enter confirm · Esc cancel
//! ```
//!
//! From ZCode's own TUI (`requestSingleChoice`, `requestMultipleChoice` and `choose` in `@zcode/tui`, 3.15.3): each question is its own list that opens on its first row. A single-choice list is the options, then `Back` from the second question on, then `Other...`; Enter on an option answers it, and Enter on `Other...` opens a text prompt whose Enter answers with the text. A multiple-choice list is the options, `Other...`, then `Back` from the second question on, then `Done`; Enter toggles a row and opens the list again on its first row, and Enter on `Done` answers. Typing filters the list, so rows are reached with Down only. There is no review step after the last question.
//! SEE-ALSO: the `zcode` arm of handle_answer_session_chat_prompt_http in server/src/session_chat_send/answer_http.rs.

use crate::session_chat::{SessionChatQuestion, SessionChatQuestionSelection};
use crate::session_chat_send::AskAnswerKeyGroup;

const ENTER: &str = "\r";
const DOWN: &str = "\u{1b}[B";
/// Part of the help line under every ZCode question list.
const ZCODE_QUESTION_HELP: &str = "Up/Down choose";

/// A screen line without its colours, frame edges and repeated spaces.
fn plain_line(line: &str) -> String {
    let line = crate::session_chat_options::strip_ansi_sgr(line);
    crate::session_chat_options::normalize_spaces(line.trim().trim_matches('│').trim())
}

/// Whether ZCode's question list is open at `questions`' first question.
pub fn zcode_question_on_screen(questions: &[SessionChatQuestion], screen_text: &str) -> bool {
    let Some(first) = questions.first() else {
        return false;
    };
    let lines: Vec<String> = screen_text.lines().map(plain_line).collect();
    let header = first
        .header
        .as_deref()
        .map(str::trim)
        .filter(|header| !header.is_empty())
        .unwrap_or("Question");
    let title = format!("{header} · 1/{}", questions.len());
    let Some(title_row) = lines.iter().rposition(|line| line == &title) else {
        return false;
    };
    let question = crate::session_chat_options::normalize_spaces(first.question.trim());
    // The question wraps under the title; joining the rows after it reads it back whole.
    let dialog = lines[title_row + 1..].join(" ");
    !question.is_empty()
        && dialog.contains(&question)
        && lines[title_row + 1..]
            .iter()
            .any(|line| line.contains(ZCODE_QUESTION_HELP))
}

fn down_then_enter(row: usize, groups: &mut Vec<AskAnswerKeyGroup>) {
    groups.extend(std::iter::repeat_n(
        AskAnswerKeyGroup::Raw(DOWN.to_string()),
        row,
    ));
    groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
}

/// Keys that answer the dialog question by question, stopping at the first question with no answer.
pub fn build_zcode_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        let back_rows = usize::from(index > 0);
        let options = question.options.len();
        let selection = selections.get(index);
        let other = selection
            .and_then(|selection| selection.other.as_deref())
            .map(str::trim)
            .unwrap_or_default();
        let mut picked: Vec<usize> = selection
            .map(|selection| selection.indices.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|row| *row < options)
            .collect();
        picked.sort_unstable();
        picked.dedup();
        if question.multi_select {
            if picked.is_empty() && other.is_empty() {
                break;
            }
            for row in &picked {
                down_then_enter(*row, &mut groups);
            }
            if !other.is_empty() {
                down_then_enter(options, &mut groups);
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
            }
            down_then_enter(options + 1 + back_rows, &mut groups);
            continue;
        }
        if !other.is_empty() {
            // A single-choice question takes one answer, so picked labels join the text.
            let mut answer: Vec<String> = picked
                .iter()
                .filter_map(|row| question.options.get(*row))
                .map(|option| option.label.clone())
                .collect();
            answer.push(other.to_string());
            down_then_enter(options + back_rows, &mut groups);
            groups.push(AskAnswerKeyGroup::Text(answer.join(", ")));
            groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
            continue;
        }
        let Some(row) = picked.first().copied() else {
            break;
        };
        down_then_enter(row, &mut groups);
    }
    groups
}
