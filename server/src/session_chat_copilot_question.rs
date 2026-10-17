//! CDXC:AgentScreenDetection 2026-10-17 WHY:
//! Copilot CLI's `ask_user` is an MCP-style elicitation: its PreToolUse hook names it `AskUserQuestion` with `{message, requestedSchema: {properties: {<field>: {title, description, enum, default}}}}`, and the terminal draws it as a form, one field at a time (measured on 1.0.98):
//!
//! ```text
//!  Copilot needs information.
//!  Which word should copilot_flow.txt start with?
//!
//!  Start word:
//!  Choose the opening word for copilot_flow.txt.
//!  ❯ hello
//!    hey
//!    Other (type your answer)
//!
//!  ↑/↓ select · enter accept · ctrl+d decline · esc cancel
//! ```
//!
//! The highlight opens on the field's default. Arrows move it one row per key and Enter accepts the highlighted row; on the Other row the typed text goes straight into the answer box under it and Enter sends it. Digits do nothing here.
//! SEE-ALSO: parse_session_chat_questions in server/src/session_chat_interactive.rs, the `copilot` arm of handle_answer_session_chat_prompt_http in server/src/session_chat_send/answer_http.rs.

use serde_json::{Map, Value};

use crate::session_chat::{
    SessionChatQuestion, SessionChatQuestionOption, SessionChatQuestionSelection,
};
use crate::session_chat_send::AskAnswerKeyGroup;

pub(crate) const COPILOT_ASK_TOOL: &str = "ask_user";
const FORM_HEADING: &str = "Copilot needs information.";
const OTHER_ROW: &str = "Other (type your answer";
const ENTER: &str = "\r";
const DOWN: &str = "\u{1b}[B";
const UP: &str = "\u{1b}[A";
/// Only the form's rule may follow its key hint.
const HINT_MAX_LINES_FROM_BOTTOM: usize = 4;

/// One card question per form field, in the order Copilot asks them. `None` when `record` is not
/// an elicitation.
pub(crate) fn parse_copilot_elicitation(
    record: &Map<String, Value>,
) -> Option<Vec<SessionChatQuestion>> {
    let schema = record.get("requestedSchema")?.as_object()?;
    let properties = schema.get("properties")?.as_object()?;
    let message = record
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let single = properties.len() == 1;
    let questions: Vec<SessionChatQuestion> = properties
        .iter()
        .filter_map(|(key, field)| {
            let field = field.as_object()?;
            let text = |name: &str| {
                field
                    .get(name)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
            };
            let title = text("title").unwrap_or_else(|| key.clone());
            let kind = field
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("string");
            let (labels, multi_select): (Vec<String>, bool) = match kind {
                "boolean" => (vec!["Yes".to_string(), "No".to_string()], false),
                "array" => (
                    enum_labels(field.get("items").and_then(|items| items.get("enum"))),
                    true,
                ),
                _ => (enum_labels(field.get("enum")), false),
            };
            let recommended = field.get("default").and_then(|default| {
                labels.iter().position(|label| match default {
                    Value::String(value) => value == label,
                    Value::Bool(true) => label == "Yes",
                    Value::Bool(false) => label == "No",
                    _ => false,
                })
            });
            let question = match (single, message) {
                (true, Some(message)) => message.to_string(),
                _ => text("description").unwrap_or_else(|| title.clone()),
            };
            Some(SessionChatQuestion {
                question,
                header: Some(title),
                multi_select,
                allow_custom: Some(kind == "string" || labels.is_empty()),
                tool_name: Some(COPILOT_ASK_TOOL.to_string()),
                recommended,
                preview_layout: false,
                options: labels
                    .into_iter()
                    .map(|label| SessionChatQuestionOption {
                        label,
                        description: None,
                        preview: None,
                    })
                    .collect(),
            })
        })
        .collect();
    (!questions.is_empty()).then_some(questions)
}

fn enum_labels(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| match value {
                    Value::String(text) => Some(text.clone()),
                    Value::Number(number) => Some(number.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The field the form has open: its option rows (the Other row last) and the highlighted one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopilotQuestionForm {
    pub rows: Vec<String>,
    pub highlighted: usize,
}

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

pub fn detect_copilot_question_form(screen_text: &str) -> Option<CopilotQuestionForm> {
    let lines: Vec<String> = screen_text
        .lines()
        .map(|line| {
            crate::session_chat_options::normalize_spaces(
                &crate::session_chat_options::strip_ansi_sgr(line),
            )
            .trim_end()
            .to_string()
        })
        .collect();
    let hint = lines
        .iter()
        .rposition(|line| line.contains("enter accept") && line.contains("esc cancel"))?;
    let trailing = lines[hint + 1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .count();
    if trailing > HINT_MAX_LINES_FROM_BOTTOM {
        return None;
    }
    let heading = lines[..hint]
        .iter()
        .rposition(|line| line.trim() == FORM_HEADING)?;
    let other = (heading..hint)
        .rev()
        .find(|&index| lines[index].contains(OTHER_ROW));
    // A field with no Other row (a yes/no question) ends at the blank line above the hint.
    let last_row = match other {
        Some(index) => index,
        None => (heading..hint)
            .rev()
            .find(|&index| !lines[index].trim().is_empty())?,
    };
    let mut first_row = last_row;
    while first_row > heading + 1 {
        let line = &lines[first_row - 1];
        let trimmed = line.trim_start();
        let is_row = trimmed.starts_with('❯') || (leading_spaces(line) >= 2 && !trimmed.is_empty());
        if !is_row {
            break;
        }
        first_row -= 1;
    }
    let rows: Vec<String> = lines[first_row..=last_row]
        .iter()
        .map(|line| line.trim().trim_start_matches('❯').trim().to_string())
        .collect();
    let highlighted = lines[first_row..=last_row]
        .iter()
        .position(|line| line.trim_start().starts_with('❯'))?;
    Some(CopilotQuestionForm { rows, highlighted })
}

fn walk(from: usize, to: usize) -> Vec<AskAnswerKeyGroup> {
    let key = if to >= from { DOWN } else { UP };
    vec![AskAnswerKeyGroup::Raw(key.to_string()); to.abs_diff(from)]
}

/// Keys that answer the card from the open field onward, or `None` when a field cannot be
/// answered from chat (a multiple-choice field, an empty answer, or a form that is not the card's).
/// The open field walks from its on-screen highlight; each later field opens on its default.
pub fn build_copilot_ask_answer_keys(
    form: &CopilotQuestionForm,
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Option<Vec<AskAnswerKeyGroup>> {
    let mut groups = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        if question.multi_select {
            return None;
        }
        let selection = selections.get(index);
        let other = selection
            .and_then(|selection| selection.other.as_deref())
            .map(str::trim)
            .filter(|text| !text.is_empty() && question.allow_custom != Some(false));
        let picked = selection
            .and_then(|selection| selection.indices.first().copied())
            .filter(|index| *index < question.options.len());
        let start = if index == 0 {
            if form.rows.len()
                != question.options.len() + usize::from(question.allow_custom == Some(true))
            {
                return None;
            }
            form.highlighted
        } else {
            question.recommended.unwrap_or(0)
        };
        match (other, picked) {
            (Some(other), _) => {
                groups.extend(walk(start, question.options.len()));
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
            }
            (None, Some(picked)) => groups.extend(walk(start, picked)),
            (None, None) => return None,
        }
        groups.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
    }
    (!groups.is_empty()).then_some(groups)
}
