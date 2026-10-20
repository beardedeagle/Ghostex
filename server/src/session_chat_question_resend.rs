//! When an agent's finished turn holds a ```questions block the chat cannot read, gxserver asks
//! the agent to send those questions again in the one shape the form reads.
//!
//! CDXC:SessionChat 2026-10-17 DECISION:
//! User: "i think for 2nd one u can tell the agent the error faced when the question list isn't right instead of supporting 1 million variations ... u should tell it to reask the question only and not write any other task but with fixed format". The questions block keeps its one canonical shape (no near-miss spellings). Once the agent's turn has ended, gxserver queues it a message (`ghostex_gx_visual::questions::resend_message`) with the parser's exact error, a small example of the shape, and the instruction to re-ask only those questions with the same Q-numbers, checked with `ghostex visual check`, and nothing else. At most once per broken block and at most two in a row for a session: a third broken retry keeps its error card and nothing more is sent.
//! SEE-ALSO: packages/gx-visual/src/questions.rs (the parser, the message and its tag), packages/gx-chat-core/src/transcript/question_forms.rs (marks the broken form as asked again) and transcript/noise.rs (folds the message), apps/desktop/src/app/native_chat/question_form.rs and apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx (the "asked again" line).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use ghostex_gx_chat_core::transcript::question_forms::{form_fences, parse_form, resend_form_id};
use ghostex_gx_visual::questions::resend_message;
use serde_json::{json, Map, Value};

use crate::logging::{GxserverLogInput, LogLevel};
use crate::presentation::read_runtime_text;
use crate::server::AppState;
use crate::session_chat::{
    SessionChatBlock, SessionChatMessage, SessionChatRole, SessionChatTailPage,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;

/// How many resend messages may follow one another before gxserver stops asking.
const MAX_IN_A_ROW: usize = 2;
/// Messages read from the transcript tail: the finished turn and the requests before it.
const SCAN_LIMIT: usize = 60;
/// The agent's last words reach its transcript a moment after the turn-end hook.
const SETTLE_DELAY: Duration = Duration::from_millis(1_500);

/// The form ids already asked about in this run, so a turn end seen twice sends once.
fn asked() -> &'static Mutex<HashSet<String>> {
    static ASKED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ASKED.get_or_init(Default::default)
}

/// Called from every place an agent's activity changes (beside the notification feed): when the
/// change is a finished turn, checks it on a background thread.
pub(crate) fn observe_turn_end(state: &AppState, params: &Map<String, Value>, result: &Value) {
    if result.get("enteredAttention").and_then(Value::as_bool) != Some(true)
        || result
            .pointer("/activity/attentionSource")
            .and_then(Value::as_str)
            != Some(TURN_COMPLETE_ATTENTION_SOURCE)
    {
        return;
    }
    let Some(session) = result.get("session").cloned() else {
        return;
    };
    let id = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .or_else(|| params.get(key).and_then(Value::as_str))
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (id("projectId"), id("sessionId")) else {
        return;
    };
    let state = state.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE_DELAY);
        check_turn(&state, &session, &project_id, &session_id);
    });
}

fn text_of(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn transcript_tail(session: &Value) -> Option<Vec<SessionChatMessage>> {
    let agent = crate::session_chat::resolve_session_chat_transcript_agent(
        session_chat_agent_for_session(session).as_deref(),
    )?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    match crate::session_chat::read_session_chat_tail_page(agent, &path, SCAN_LIMIT, None).ok()? {
        SessionChatTailPage::Page { messages, .. } => Some(messages),
        _ => None,
    }
}

fn check_turn(state: &AppState, session: &Value, project_id: &str, session_id: &str) {
    let Some(messages) = transcript_tail(session) else {
        return;
    };
    let turn_start = messages
        .iter()
        .rposition(|message| message.role == SessionChatRole::User && !message.queued)
        .map_or(0, |index| index + 1);
    let user_texts: Vec<String> = messages[..turn_start]
        .iter()
        .filter(|message| message.role == SessionChatRole::User && !message.queued)
        .map(text_of)
        .collect();
    // The requests that started the latest turns, newest first: resend after resend means the
    // agent keeps failing, and gxserver stops after MAX_IN_A_ROW.
    let in_a_row = user_texts
        .iter()
        .rev()
        .take_while(|text| resend_form_id(text).is_some())
        .count();
    if in_a_row >= MAX_IN_A_ROW {
        return;
    }
    let already_asked: Vec<&str> = user_texts
        .iter()
        .filter_map(|text| resend_form_id(text))
        .collect();
    let broken = messages[turn_start..]
        .iter()
        .filter(|message| message.role == SessionChatRole::Assistant && !message.queued)
        .flat_map(|message| form_fences(&text_of(message)))
        .map(|(_, body)| parse_form(&body))
        .find(|form| !form.error.is_empty() && !already_asked.contains(&form.id.as_str()));
    let Some(form) = broken else {
        return;
    };
    if !asked()
        .lock()
        .map(|mut asked| asked.insert(form.id.clone()))
        .unwrap_or(false)
    {
        return;
    }
    // The queue sends it once the agent is idle and nothing waits on its screen, so it never
    // interrupts a turn, with or without a chat open.
    let outcome = crate::server::queue_session_chat_prompt(
        state,
        project_id,
        session_id,
        &resend_message(&form),
        false,
    );
    if let Err(error) = outcome {
        let _ = state.logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "sessionChat.questionsResendFailed".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(error.message),
            details: Some(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "formId": form.id,
            })),
        });
    }
}
