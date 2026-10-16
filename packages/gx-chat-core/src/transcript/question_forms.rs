//! Finds the ```questions blocks in an agent's replies. The grammar itself (what a block may say,
//! its id, the answers message) is `ghostex_gx_visual::questions`, which `ghostex visual check`
//! reads too.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! native_markdown.rs marks the fence with the form's id; questions/forms.rs keeps the reader's picks and sends them; apps/desktop/src/app/native_chat/question_form.rs and apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx draw it; skills/ghostex-visuals/SKILL.md teaches the grammar. Change them together.

use ghostex_gx_protocol::{ChatMessage, ChatRole};

pub use ghostex_gx_visual::questions::{
    answers_message, form_id, is_questions_language, notes_key, parse_form, takes_notes,
    FormChoice, FormQuestion, QuestionForm, NOTE_SUFFIX, OTHER_CHOICE, SKIPPED,
};

use crate::transcript::jsstr::{js_trim, split_lf};
use crate::transcript::native_markdown::{fence_close, fence_open};
use crate::transcript::prose::prose_markdown;

/// The finished fences in one message's Markdown, as `(language, body)`: a fence still streaming
/// is not one yet.
pub fn finished_fences(markdown: &str) -> Vec<(String, String)> {
    let lines = split_lf(markdown);
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some((character, run, info)) = fence_open(lines[index]) else {
            index += 1;
            continue;
        };
        let close = lines[index + 1..]
            .iter()
            .position(|line| {
                fence_close(line)
                    .is_some_and(|(closing, length)| closing == character && length >= run)
            })
            .map(|offset| index + 1 + offset);
        let Some(close) = close else {
            break;
        };
        let language = js_trim(info)
            .split(char::is_whitespace)
            .next()
            .unwrap_or_default();
        found.push((language.to_string(), lines[index + 1..close].join("\n")));
        index = close + 1;
    }
    found
}

/// The finished ```questions fences in one message's Markdown, as `(id, body)`.
pub fn form_fences(markdown: &str) -> Vec<(String, String)> {
    finished_fences(markdown)
        .into_iter()
        .filter(|(language, _)| is_questions_language(language))
        .map(|(_, body)| (form_id(&body), body))
        .collect()
}

/// Every form the agent's replies hold, oldest first, once each. A block that only withdraws
/// earlier questions (`open_questions.rs`) is not a form.
pub fn question_forms(messages: &[ChatMessage]) -> Vec<QuestionForm> {
    let mut forms: Vec<QuestionForm> = Vec::new();
    for message in messages
        .iter()
        .filter(|message| message.role == ChatRole::Assistant)
    {
        for (id, body) in form_fences(&prose_markdown(&message.blocks)) {
            if !forms.iter().any(|form| form.id == id) {
                let form = parse_form(&body);
                if !form.is_withdrawal() {
                    forms.push(form);
                }
            }
        }
    }
    forms
}
