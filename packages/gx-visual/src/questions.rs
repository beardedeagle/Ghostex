//! The grammar of a ```questions block: the form an agent writes in its reply, which the chat draws
//! as a form (packages/gx-chat-core/src/transcript/question_forms.rs finds the blocks, and
//! `ghostex visual check` reads one the same way).
//!
//! ```text
//! ```questions
//! {
//!   "title": "Open questions",
//!   "questions": [
//!     {"tag": "Q1", "title": "Which layout?", "detail": "Why it matters.",
//!      "choices": [{"label": "Tabs", "detail": "One page per area.", "recommended": true},
//!                  "One long page"]},
//!     {"title": "Which platforms first?", "multiple": true, "choices": ["macOS", "Windows"]},
//!     {"title": "Anything else?", "type": "text"}
//!   ]
//! }
//! ```
//! ```
//!
//! The model is the page kit's (`server/src/visual_pages/kit.rs`): a choice question ends with
//! "Other / none of these" and a note box, nothing is preselected, and the answers go back to the
//! agent as the same `<ghostex-page-answers>` message a page sends, keyed by each question's `name`
//! (its title when it has none), a note as `"<key> (note)"`.
//!
//! CDXC:SessionChat 2026-10-11 DECISION:
//! User: questions an agent asks are answered in a form inside the chat itself (desktop and phone), not one question at a time and not by typing letters: all questions at once, selectable choices with nothing preselected (clicking again clears), the automatic "Other / none of these" with a note box, text questions, and one Send button. Any agent writes it as a ```questions block, the answers go back as a message, and the form then shows as answered. Pages (gx-questions) stay only for questions that need visuals.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! packages/gx-chat-core/src/transcript/native_markdown.rs marks the fence with this id; packages/gx-chat-core/src/questions/forms.rs keeps the reader's picks and sends them; apps/desktop/src/app/native_chat/question_form.rs and apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx draw it; server/src/ghostex_cli/visual.rs checks a block; skills/ghostex-visuals/SKILL.md teaches the grammar. Change them together.

use serde::Serialize;
use serde_json::{Map, Value};

/// The choice every choice question ends with.
pub const OTHER_CHOICE: &str = "Other / none of these";
/// How a question's note is keyed in the answers.
pub const NOTE_SUFFIX: &str = " (note)";

const MAX_QUESTIONS: usize = 20;
const MAX_CHOICES: usize = 26;

/// Whether a fence's language names a questions block.
pub fn is_questions_language(language: &str) -> bool {
    language.trim().eq_ignore_ascii_case("questions")
}

/// One form, as the renderers draw it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionForm {
    pub id: String,
    pub title: String,
    /// The send button's label.
    pub submit: String,
    pub questions: Vec<FormQuestion>,
    /// The tags of earlier questions this block withdraws (`"withdraw": ["Q40"]`): they no longer
    /// count as unanswered. A block may hold only this, with no questions of its own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub withdraw: Vec<String>,
    /// Why the block could not be read, shown in place of the form.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

impl QuestionForm {
    /// A block that only withdraws earlier questions and asks none: the chat draws a line naming
    /// the withdrawn tags instead of a form.
    pub fn is_withdrawal(&self) -> bool {
        self.error.is_empty() && self.questions.is_empty() && !self.withdraw.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormQuestion {
    /// The answer's key: the question's `name`, else its title.
    pub key: String,
    pub tag: String,
    pub title: String,
    pub detail: String,
    pub multiple: bool,
    /// A text box instead of choices.
    pub text: bool,
    pub placeholder: String,
    pub choices: Vec<FormChoice>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormChoice {
    pub label: String,
    pub detail: String,
    /// What the answer carries: `value`, else the label.
    pub value: String,
    pub recommended: bool,
}

/// FNV-1a over `text`, from `basis`.
fn fnv(text: &str, basis: u64) -> u64 {
    text.bytes().fold(basis, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The form's id: 32 hex characters from its JSON with the spacing taken out, so every client (and
/// the answers message) names the same form the same way, however the reply was wrapped.
pub fn form_id(body: &str) -> String {
    let canonical = serde_json::from_str::<Value>(body)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| body.trim().to_string());
    format!(
        "{:016x}{:016x}",
        fnv(&canonical, 0xcbf2_9ce4_8422_2325),
        fnv(&canonical, 0x6c62_272e_07bb_0142)
    )
}

fn string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

fn choice(value: &Value, number: usize, question: usize) -> Result<FormChoice, String> {
    let (label, object) = match value {
        Value::String(label) => (label.trim().to_string(), None),
        Value::Object(object) => (string(object.get("label")), Some(object)),
        _ => {
            return Err(format!(
                "Choice {number} of question {question} must be text or an object with a \"label\"."
            ))
        }
    };
    if label.is_empty() {
        return Err(format!(
            "Choice {number} of question {question} needs a \"label\"."
        ));
    }
    let value = object
        .map(|object| string(object.get("value")))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| label.clone());
    Ok(FormChoice {
        detail: object
            .map(|object| string(object.get("detail").or_else(|| object.get("description"))))
            .unwrap_or_default(),
        recommended: object
            .and_then(|object| object.get("recommended"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        label,
        value,
    })
}

fn question(
    object: &Map<String, Value>,
    number: usize,
    used: &mut Vec<String>,
) -> Result<FormQuestion, String> {
    let title = string(object.get("title").or_else(|| object.get("question")));
    if title.is_empty() {
        return Err(format!("Question {number} needs a \"title\"."));
    }
    let text = string(object.get("type")).eq_ignore_ascii_case("text");
    let choices = match object.get("choices").or_else(|| object.get("options")) {
        Some(Value::Array(choices)) if !text => choices
            .iter()
            .enumerate()
            .map(|(index, value)| choice(value, index + 1, number))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };
    if !text && choices.is_empty() {
        return Err(format!(
            "Question {number} needs \"choices\", or \"type\": \"text\" for a text answer."
        ));
    }
    if choices.len() > MAX_CHOICES {
        return Err(format!(
            "Question {number} has {} choices; a question takes at most {MAX_CHOICES}.",
            choices.len()
        ));
    }
    let base = Some(string(object.get("name")))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| title.clone());
    let mut key = base.clone();
    let mut copy = 2;
    while used.contains(&key) {
        key = format!("{base} ({copy})");
        copy += 1;
    }
    used.push(key.clone());
    Ok(FormQuestion {
        key,
        tag: string(object.get("tag")),
        detail: string(object.get("detail").or_else(|| object.get("description"))),
        multiple: object
            .get("multiple")
            .or_else(|| object.get("multiSelect"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        placeholder: string(object.get("placeholder")),
        text,
        choices,
        title,
    })
}

/// A block's `withdraw` list: the tags of earlier questions it withdraws.
///
/// CDXC:SessionChat 2026-10-14 DECISION:
/// User (Q82): questions the agent marks obsolete drop off the chat's unanswered count. A later form or page that asks the same tag supersedes the earlier question, and `"withdraw": ["Q40"]` in a later ```questions block (alone, or beside new questions) withdraws a tag without asking anything.
fn withdrawn_tags(value: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or("\"withdraw\" must be a list of question tags, such as [\"Q40\"].")?;
    let mut tags: Vec<String> = Vec::new();
    for item in items {
        let tag = item
            .as_str()
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .ok_or("Each entry in \"withdraw\" must be a question tag, such as \"Q40\".")?;
        if !tags.iter().any(|known| known.eq_ignore_ascii_case(tag)) {
            tags.push(tag.to_string());
        }
    }
    Ok(tags)
}

/// Reads one block. A block that cannot be read is still a form, with `error` saying why, so the
/// chat shows the problem in place of the questions.
pub fn parse_form(body: &str) -> QuestionForm {
    let id = form_id(body);
    let read = || -> Result<QuestionForm, String> {
        let value: Value = serde_json::from_str(body)
            .map_err(|error| format!("The questions block is not valid JSON: {error}."))?;
        let object = value
            .as_object()
            .ok_or("A questions block is one JSON object with a \"questions\" list.")?;
        let withdraw = withdrawn_tags(object.get("withdraw"))?;
        let list = match object.get("questions") {
            None if !withdraw.is_empty() => Vec::new(),
            questions => questions
                .and_then(Value::as_array)
                .filter(|list| !list.is_empty())
                .cloned()
                .ok_or(
                    "A questions block needs a \"questions\" list with at least one question.",
                )?,
        };
        if list.len() > MAX_QUESTIONS {
            return Err(format!(
                "The block asks {} questions; a form takes at most {MAX_QUESTIONS}.",
                list.len()
            ));
        }
        let mut used = Vec::new();
        let questions = list
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_object()
                    .ok_or_else(|| format!("Question {} must be an object.", index + 1))
                    .and_then(|object| question(object, index + 1, &mut used))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(QuestionForm {
            id: id.clone(),
            title: Some(string(object.get("title")))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "Questions".to_string()),
            submit: Some(string(object.get("submit")))
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| "Send answers".to_string()),
            questions,
            withdraw,
            error: String::new(),
        })
    };
    read().unwrap_or_else(|error| QuestionForm {
        id: id.clone(),
        title: "Questions".to_string(),
        error,
        ..QuestionForm::default()
    })
}

/// The message that carries a form's answers to the agent: the envelope a published page's
/// answers use (`server/src/visual_pages/answers.rs`), so the agent reads both the same way and
/// the chat draws both as an answers card.
pub fn answers_message(form: &QuestionForm, answers: &Value) -> String {
    page_answers_message(&form.id, &form.title, answers)
}

/// The `<ghostex-page-answers page="…" title="…">` message, for a question form or a published page
/// alike (server/src/visual_pages/answers.rs).
///
/// CDXC:SessionChat 2026-10-11 WHY:
/// The agent reads this envelope as text, so the title keeps its own characters ("Interrupt & Send
/// follow-ups", not "&amp;"): only a quote, an angle bracket, a line break and an ampersand that
/// would read as one of these escapes are escaped, which is what the chat's parser
/// (packages/gx-chat-core/src/transcript/page_answers.rs) needs to read the title back exactly.
pub fn page_answers_message(id: &str, title: &str, answers: &Value) -> String {
    let body = serde_json::to_string_pretty(answers).unwrap_or_else(|_| "null".to_string());
    format!(
        "<ghostex-page-answers page=\"{id}\" title=\"{}\">\n{body}\n</ghostex-page-answers>",
        escape_title(title)
    )
}

fn escape_title(title: &str) -> String {
    let mut escaped = String::with_capacity(title.len());
    for (at, character) in title.char_indices() {
        match character {
            '&' if ["&amp;", "&quot;", "&lt;", "&gt;"]
                .iter()
                .any(|entity| title[at..].starts_with(entity)) =>
            {
                escaped.push_str("&amp;")
            }
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\r' | '\n' => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}
