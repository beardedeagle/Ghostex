//! The questions an agent asked in its chat, for the voice agent: every open ```questions form and
//! question page (`ghostex show`) of a session, listed with a ref for each question (its tag, like
//! Q335) and answered the way the reader would: a form's answers go into the session's chat queue as
//! the same `<ghostex-page-answers>` message the chat's Send answers sends (built by gx-chat-core's
//! own `forms::answers`), and a page's answers go through the page's own answers route, so the
//! agent cannot tell who answered and the chat marks the form or page Answered.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User (as the voice agent): "I tried to answer your inline question Q335 using the answer tool, but it returned that nothing was waiting for an answer ... Please adjust ... so the voice agent can submit an answer to them directly." get_pending_prompt lists a session's open forms and question pages (the same ones its pink question dot counts), and answer_prompt answers them by each question's ref. Questions of a form or page that the user did not answer are sent as skipped, exactly like a blank question when the reader presses Send answers; forms and pages the user did not touch stay open.
//! SEE-ALSO: packages/gx-chat-core/src/questions/forms.rs (`answers`, the form's keys), server/src/visual_pages/kit.rs (the page's keys and values), server/src/visual_pages/questions.rs, server/src/session_chat_open_questions.rs.

use std::sync::Arc;
use std::time::Duration;

use ghostex_gx_chat_core::questions::forms::answers as form_answers;
use ghostex_gx_chat_core::transcript::open_questions::AskerKind;
use ghostex_gx_chat_core::transcript::question_forms::{
    answers_message, takes_notes, QuestionForm, NOTE_SUFFIX, OTHER_CHOICE, SKIPPED,
};
use ghostex_gx_chat_core::QuestionDraft;
use serde_json::{json, Map, Value};

use super::loopback;
use super::sessions::SessionInfo;
use crate::server::AppState;

/// One choice as the voice agent hears it.
struct Choice {
    label: String,
    detail: String,
    /// What a page's answers carry for it (a form's are built by gx-chat-core).
    value: String,
    recommended: bool,
}

struct Question {
    /// What the voice agent names it by: its tag, else "<form>.<question>".
    reference: String,
    /// The page's answers key (unused for a form, whose keys gx-chat-core builds).
    key: String,
    title: String,
    detail: String,
    multiple: bool,
    text: bool,
    choices: Vec<Choice>,
}

/// One open form or question page.
struct Asked {
    id: String,
    kind: AskerKind,
    title: String,
    form: Option<QuestionForm>,
    questions: Vec<Question>,
}

/// The session's open forms and question pages, oldest first. Blocking.
fn collect(state: &AppState, project_id: &str, session_id: &str) -> Vec<Asked> {
    let mut asked: Vec<Asked> = Vec::new();
    for asker in crate::session_chat_open_questions::open_askers(state, project_id, session_id) {
        match (&asker.kind, asker.form) {
            (AskerKind::Form, Some(form)) => asked.push(Asked {
                id: asker.id,
                kind: AskerKind::Form,
                title: form.title.clone(),
                questions: form
                    .questions
                    .iter()
                    .map(|question| Question {
                        reference: question.tag.clone(),
                        key: question.key.clone(),
                        title: question.title.clone(),
                        detail: question.detail.clone(),
                        multiple: question.multiple,
                        text: question.text,
                        choices: question
                            .choices
                            .iter()
                            .map(|choice| Choice {
                                label: choice.label.clone(),
                                detail: choice.detail.clone(),
                                value: choice.value.clone(),
                                recommended: choice.recommended,
                            })
                            .collect(),
                    })
                    .collect(),
                form: Some(form),
            }),
            (AskerKind::Page, _) => {
                let Some((title, questions)) =
                    crate::visual_pages::page_questions(&state.paths.root_dir, &asker.id)
                else {
                    continue;
                };
                asked.push(Asked {
                    id: asker.id,
                    kind: AskerKind::Page,
                    title,
                    form: None,
                    questions: questions
                        .into_iter()
                        .map(|question| Question {
                            reference: question.tag,
                            key: question.key,
                            title: question.title,
                            detail: String::new(),
                            multiple: question.multiple,
                            text: question.text,
                            choices: question
                                .choices
                                .into_iter()
                                .map(|choice| Choice {
                                    label: choice.label,
                                    detail: String::new(),
                                    value: choice.value,
                                    recommended: choice.recommended,
                                })
                                .collect(),
                        })
                        .collect(),
                });
            }
            _ => {}
        }
    }
    // A tag names its question only when no other open question uses it.
    let tags: Vec<String> = asked
        .iter()
        .flat_map(|asked| asked.questions.iter())
        .map(|question| question.reference.to_ascii_uppercase())
        .collect();
    for (number, entry) in asked.iter_mut().enumerate() {
        for (index, question) in entry.questions.iter_mut().enumerate() {
            let tag = question.reference.trim().to_ascii_uppercase();
            if tag.is_empty() || tags.iter().filter(|known| **known == tag).count() > 1 {
                question.reference = format!("{}.{}", number + 1, index + 1);
            }
        }
    }
    asked
}

async fn collect_async(state: &Arc<AppState>, session: &SessionInfo) -> Vec<Asked> {
    let state = state.clone();
    let (project_id, session_id) = (session.project_id.clone(), session.session_id.clone());
    tokio::task::spawn_blocking(move || collect(&state, &project_id, &session_id))
        .await
        .unwrap_or_default()
}

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// The session's open forms and question pages, for get_pending_prompt. Empty when none wait.
pub(super) async fn listing(state: &Arc<AppState>, session: &SessionInfo) -> Vec<Value> {
    collect_async(state, session)
        .await
        .iter()
        .enumerate()
        .map(|(number, asked)| {
            json!({
                "number": number + 1,
                "kind": if asked.kind == AskerKind::Page { "page" } else { "form" },
                "title": short(&asked.title, 200),
                "questions": asked.questions.iter().map(|question| {
                    let mut entry = json!({
                        "ref": question.reference,
                        "question": short(&question.title, 400),
                        "kind": if question.text { "text" } else if question.multiple { "several choices" } else { "one choice" },
                    });
                    if !question.detail.is_empty() {
                        entry["detail"] = json!(short(&question.detail, 300));
                    }
                    if !question.text {
                        entry["options"] = json!(question.choices.iter().enumerate().map(|(n, choice)| {
                            let mut option = json!({"number": n + 1, "label": short(&choice.label, 200)});
                            if !choice.detail.is_empty() {
                                option["detail"] = json!(short(&choice.detail, 200));
                            }
                            if choice.recommended {
                                option["recommended"] = json!(true);
                            }
                            option
                        }).collect::<Vec<_>>());
                    }
                    entry
                }).collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// Whether the call names questions by ref, so it is meant for a form or page.
pub(super) fn names_questions(args: &Value) -> bool {
    args.get("answers")
        .and_then(Value::as_array)
        .is_some_and(|answers| {
            answers
                .iter()
                .any(|answer| answer.get("question").is_some())
        })
}

/// What the user said for one question.
#[derive(Default)]
struct Given {
    options: Vec<usize>,
    /// A free answer: a text question's answer, or "Other / none of these" with this note.
    text: String,
    note: String,
    skip: bool,
}

fn text_arg(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn draft(question: &Question, given: Option<&Given>) -> QuestionDraft {
    let mut draft = QuestionDraft::default();
    let Some(given) = given else {
        draft.skipped = true;
        return draft;
    };
    draft.indices = given.options.iter().map(|index| *index as u32).collect();
    if question.text {
        draft.other = if given.text.is_empty() {
            given.note.clone()
        } else {
            given.text.clone()
        };
    } else if !given.text.is_empty() {
        draft.other_picked = true;
        draft.other = given.text.clone();
    } else {
        draft.other = given.note.clone();
    }
    // A question with nothing given goes as skipped, like a blank one in the chat's form.
    draft.skipped = given.skip
        || (draft.indices.is_empty() && !draft.other_picked && draft.other.trim().is_empty());
    draft
}

/// A page's answers, keyed and valued like the page kit's `collect` (kit.rs).
fn page_answers(asked: &Asked, given: &[Option<&Given>]) -> Value {
    let mut out = Map::new();
    for (question, given) in asked.questions.iter().zip(given) {
        let given = given.filter(|given| !given.skip);
        if question.text {
            let answer = given
                .map(|given| {
                    if given.text.is_empty() {
                        given.note.clone()
                    } else {
                        given.text.clone()
                    }
                })
                .filter(|answer| !answer.is_empty());
            out.insert(
                question.key.clone(),
                json!(answer.unwrap_or_else(|| SKIPPED.to_string())),
            );
            continue;
        }
        let Some(given) = given else {
            out.insert(question.key.clone(), json!(SKIPPED));
            continue;
        };
        let mut picked: Vec<Value> = given
            .options
            .iter()
            .filter_map(|index| question.choices.get(*index))
            .map(|choice| json!(choice.value))
            .collect();
        let mut note = given.note.clone();
        if !given.text.is_empty() {
            picked.push(json!(OTHER_CHOICE));
            note = given.text.clone();
        }
        let value = match (picked.is_empty(), question.multiple) {
            (false, true) => Value::Array(picked),
            (false, false) => picked.remove(0),
            (true, _) if !note.is_empty() => Value::Null,
            (true, _) => json!(SKIPPED),
        };
        out.insert(question.key.clone(), value);
        if !note.is_empty() {
            out.insert(format!("{}{NOTE_SUFFIX}", question.key), json!(note));
        }
    }
    Value::Object(out)
}

/// Answers questions of the session's open forms and pages by ref. Each form or page that gets an
/// answer is sent, its other questions as skipped; the others stay open.
pub(super) async fn answer(
    state: &Arc<AppState>,
    session: &SessionInfo,
    args: &Value,
) -> Result<Value, String> {
    let asked = collect_async(state, session).await;
    if asked.is_empty() {
        return Err("Nothing is waiting for an answer in that session any more.".to_string());
    }
    let refs = || {
        asked
            .iter()
            .flat_map(|asked| {
                asked
                    .questions
                    .iter()
                    .map(|question| question.reference.clone())
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    // (form or page, question) -> what the user said.
    let mut given: Vec<Vec<Option<Given>>> = asked
        .iter()
        .map(|asked| asked.questions.iter().map(|_| None).collect())
        .collect();
    for entry in args
        .get("answers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let wanted = text_arg(entry, "question");
        let found = asked.iter().enumerate().find_map(|(a, asked)| {
            asked
                .questions
                .iter()
                .position(|question| question.reference.eq_ignore_ascii_case(&wanted))
                .map(|q| (a, q))
        });
        let Some((a, q)) = found else {
            return Err(format!(
                "No open question is called \"{wanted}\". Open questions: {}.",
                refs()
            ));
        };
        let question = &asked[a].questions[q];
        let options: Vec<usize> = entry
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .filter(|number| *number >= 1)
            .map(|number| number as usize - 1)
            .collect();
        if options.iter().any(|index| *index >= question.choices.len()) {
            return Err(format!(
                "{} has only {} options.",
                question.reference,
                question.choices.len()
            ));
        }
        if options.len() > 1 && !question.multiple {
            return Err(format!("{} takes one option.", question.reference));
        }
        given[a][q] = Some(Given {
            options,
            text: text_arg(entry, "text"),
            note: text_arg(entry, "note"),
            skip: entry.get("skip").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    let notes = text_arg(args, "notes");
    let mut sent: Vec<Value> = Vec::new();
    for (asked, given) in asked.iter().zip(&given) {
        if given.iter().all(Option::is_none) {
            continue;
        }
        let picks = |question: &Question, given: &Option<Given>| -> Vec<String> {
            match given {
                None => vec!["skipped".to_string()],
                Some(given) if given.skip => vec!["skipped".to_string()],
                Some(given) => {
                    let mut said: Vec<String> = given
                        .options
                        .iter()
                        .filter_map(|index| question.choices.get(*index))
                        .map(|choice| choice.label.clone())
                        .collect();
                    if !given.text.is_empty() {
                        said.push(given.text.clone());
                    }
                    if said.is_empty() {
                        said.push("skipped".to_string());
                    }
                    said
                }
            }
        };
        let said: Vec<Value> = asked
            .questions
            .iter()
            .zip(given)
            .map(|(question, given)| json!({"question": question.reference, "answer": picks(question, given).join(", ")}))
            .collect();
        match (&asked.kind, &asked.form) {
            (AskerKind::Form, Some(form)) => {
                let drafts: Vec<QuestionDraft> = asked
                    .questions
                    .iter()
                    .zip(given)
                    .map(|(question, given)| draft(question, given.as_ref()))
                    .collect();
                let notes = if takes_notes(form) {
                    notes.as_str()
                } else {
                    ""
                };
                let answers = form_answers(form, &drafts, notes);
                let millis = chrono::Utc::now().timestamp_millis();
                // The chat's Send answers queues the same message (gx-chat-core questions/forms.rs `send`).
                loopback::call(
                    state,
                    "/api/queueSessionChatPrompt",
                    json!({
                        "projectId": session.project_id,
                        "sessionId": session.session_id,
                        "text": answers_message(form, &answers),
                        "sendRequestId": format!("question-form-{}-{millis}", form.id),
                    }),
                    Duration::from_secs(30),
                )
                .await?;
            }
            _ => {
                let given: Vec<Option<&Given>> = given.iter().map(Option::as_ref).collect();
                let body = json!({"answers": page_answers(asked, &given)}).to_string();
                let routed = crate::visual_pages::submit_answers(
                    state.clone(),
                    format!(
                        "{}{}/answers",
                        crate::visual_pages::VISUAL_PAGE_ROUTE_PREFIX,
                        asked.id
                    ),
                    axum::body::Body::from(body),
                )
                .await;
                let status = routed.response.status();
                let reply = axum::body::to_bytes(routed.response.into_body(), 64 * 1024)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .unwrap_or(Value::Null);
                if !status.is_success() || reply.get("ok").and_then(Value::as_bool) != Some(true) {
                    return Err(reply
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("The page did not take the answers.")
                        .to_string());
                }
            }
        }
        sent.push(json!({"title": short(&asked.title, 200), "answers": said}));
    }
    if sent.is_empty() {
        return Err(format!(
            "Say which questions to answer by ref. Open questions: {}.",
            refs()
        ));
    }
    // The pink question dot follows at once instead of at the next sweep.
    let refresh_state = state.clone();
    let (project_id, session_id) = (session.project_id.clone(), session.session_id.clone());
    let _ = tokio::task::spawn_blocking(move || {
        crate::session_chat_open_questions::refresh(&refresh_state, &project_id, &session_id, None)
    })
    .await;
    Ok(json!({"ok": true, "answered": true, "session": session.brief(), "sent": sent}))
}
