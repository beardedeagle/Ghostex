//! Whether a session's agent ended its turn waiting on the user's answers: its reply holds a
//! ```questions form or a question page (`ghostex show` with questions) that nothing has answered
//! yet. The forms and pages are read by gx-chat-core's own parser (the one that counts the chat's
//! open questions), so the sidebar and the chat never disagree on what was asked.
//!
//! CDXC:SessionStatus 2026-10-20 DECISION:
//! User: "when a session has open questions we must show an indicator in the sidebar and as notification ... i didn't know this session needed answers from me which is bad". When a turn ends and its reply asks questions nobody has answered, gxserver records them on the session (`sessionChatOpenQuestions`) and the presentation adds them to `pendingQuestionCount`, so every client draws the pink question dot ("Answer requested") that Codex's own questions already use; it is never cleared by just looking at the session. It clears when the user sends the session anything (the answers included), since that starts a new turn, and every turn end decides again, so a later turn that withdraws or asks again replaces it. Orchestrator threads are left out: their questions go to their orchestrator in its thread report.
//! CDXC:Notifications 2026-10-20 DECISION:
//! Same request: one notification per turn that asked. A turn end that already notified (the agent's Stop entered attention) has its feed row turned from Finished into Needs input, saying how many questions wait; a turn that ended without attention (agents whose finished turn is idle) is raised to attention once, which sounds, shows the system notification and writes the feed row like any other wait for input.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/open_questions.rs (what counts as asked, answered and withdrawn), server/src/presentation/session_projection.rs (`pendingQuestionCount`), server/src/session_chat_question_resend.rs (the sibling turn-end check for broken forms).

use std::time::Duration;

use ghostex_gx_chat_core::protocol::ChatMessage;
use ghostex_gx_chat_core::transcript::open_questions::{askers, AskerKind};
use ghostex_gx_chat_core::transcript::question_forms::question_forms;
use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::AppState;
use crate::session_chat::{SessionChatMessage, SessionChatRole};
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;
use crate::storage::open_gxserver_database;

/// Where the open questions of the last finished turn are kept on the session.
const RUNTIME_KEY: &str = "sessionChatOpenQuestions";
/// The agent's last words reach its transcript a moment after the turn-end hook.
const SETTLE_DELAY: Duration = Duration::from_millis(1_500);

/// The questions a finished turn left waiting.
#[derive(Debug, PartialEq)]
struct OpenTurnQuestions {
    /// The forms' and pages' ids, which tell one turn's questions from the next.
    key: String,
    /// How many questions wait: a form's questions, a page's tags (at least one per page).
    count: usize,
    /// The first form question's title, for the notification.
    first_title: Option<String>,
}

/// How many questions the session's last finished turn left waiting, as recorded at its end.
pub(crate) fn open_question_count(session: &Value) -> usize {
    session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}/count"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

/// Called from every place an agent's activity changes, beside the notification feed: a turn
/// that starts clears the record, and a turn that ends is checked on a background thread.
pub(crate) fn observe_activity(state: &AppState, params: &Map<String, Value>, result: &Value) {
    let Some(session) = result.get("session") else {
        return;
    };
    let previous = result
        .get("previousActivity")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let next = result
        .pointer("/activity/activity")
        .and_then(Value::as_str)
        .unwrap_or("idle");
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
    if next == "working" {
        if previous != "working" && open_question_count(session) > 0 {
            if let Ok(db) = open_gxserver_database(&state.paths) {
                let _ = write_record(state, &db, &project_id, &session_id, None);
            }
        }
        return;
    }
    let entered_turn_end_attention = result.get("enteredAttention").and_then(Value::as_bool)
        == Some(true)
        && result
            .pointer("/activity/attentionSource")
            .and_then(Value::as_str)
            == Some(TURN_COMPLETE_ATTENTION_SOURCE);
    let ended_idle = previous == "working" && next == "idle";
    if !entered_turn_end_attention && !ended_idle {
        return;
    }
    let state = state.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE_DELAY);
        check_turn(&state, &project_id, &session_id, ended_idle);
    });
}

fn check_turn(state: &AppState, project_id: &str, session_id: &str, ended_idle: bool) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    // A new turn started while the transcript settled; its own end decides.
    if crate::presentation::presentation_activity(&session, &crate::domain::now_iso()) == "working"
    {
        return;
    }
    let is_thread = crate::coordinators::read_thread(&db, project_id, session_id)
        .ok()
        .flatten()
        .is_some_and(|thread| !thread.is_resolved());
    let open = if is_thread {
        None
    } else {
        crate::session_chat_question_resend::transcript_tail(&session)
            .and_then(|messages| open_turn_questions(&messages))
    };
    let recorded = session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}/key"))
        .and_then(Value::as_str);
    let newly_asked = open
        .as_ref()
        .is_some_and(|open| recorded != Some(open.key.as_str()));
    if recorded.is_none() && open.is_none() {
        return;
    }
    if open.as_ref().map(|open| open.key.as_str()) != recorded
        && write_record(state, &db, project_id, session_id, open.as_ref()).is_err()
    {
        return;
    }
    if let Some(open) = open {
        notify(
            state,
            &db,
            &repository,
            project_id,
            session_id,
            &open,
            ended_idle && newly_asked,
        );
    }
}

/// The questions the latest finished turn asked that nothing answered or withdrew: its replies
/// since the last message the user (or anyone) sent, read by gx-chat-core's parser.
fn open_turn_questions(messages: &[SessionChatMessage]) -> Option<OpenTurnQuestions> {
    let turn_start = messages
        .iter()
        .rposition(|message| message.role == SessionChatRole::User && !message.queued)
        .map_or(0, |index| index + 1);
    let turn: Vec<ChatMessage> = messages[turn_start..]
        .iter()
        .filter(|message| message.role == SessionChatRole::Assistant && !message.queued)
        .filter_map(|message| {
            serde_json::to_value(message)
                .ok()
                .and_then(|value| serde_json::from_value(value).ok())
        })
        .collect();
    if turn.is_empty() {
        return None;
    }
    let forms = question_forms(&turn);
    let open: Vec<_> = askers(&turn, &[])
        .into_iter()
        .filter(|asker| !asker.answered && !asker.obsolete)
        .collect();
    if open.is_empty() {
        return None;
    }
    let form = |id: &str| forms.iter().find(|form| form.id == id);
    let count = open
        .iter()
        .map(|asker| match asker.kind {
            AskerKind::Form => form(&asker.id).map_or(1, |form| form.questions.len().max(1)),
            AskerKind::Page => asker.tags.len().max(1),
        })
        .sum();
    let first_title = open
        .iter()
        .filter(|asker| asker.kind == AskerKind::Form)
        .find_map(|asker| form(&asker.id)?.questions.first().map(|q| q.title.clone()))
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty());
    Some(OpenTurnQuestions {
        key: open
            .iter()
            .map(|asker| asker.id.as_str())
            .collect::<Vec<_>>()
            .join(","),
        count,
        first_title,
    })
}

/// Records (or, with `None`, clears) the turn's open questions and tells every client.
fn write_record(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    open: Option<&OpenTurnQuestions>,
) -> anyhow::Result<()> {
    let changed = match open {
        Some(open) => db.execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatOpenQuestions', json(?3))
            WHERE projectId = ?1 AND sessionId = ?2"#,
            rusqlite::params![
                project_id,
                session_id,
                json!({ "key": open.key, "count": open.count }).to_string()
            ],
        )?,
        None => db.execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.sessionChatOpenQuestions')
            WHERE projectId = ?1 AND sessionId = ?2
            AND json_extract(runtimeSettingsJson, '$.sessionChatOpenQuestions') IS NOT NULL"#,
            rusqlite::params![project_id, session_id],
        )?,
    };
    if changed > 0 {
        let repository = DomainRepository::new(db, state.metadata.server_id.as_str());
        crate::server::schedule_presentation_session_delta(
            state,
            db,
            &repository,
            project_id,
            session_id,
        )?;
    }
    Ok(())
}

fn notification_body(open: &OpenTurnQuestions) -> String {
    let waiting = match open.count {
        1 => "1 question is waiting for your answer".to_string(),
        count => format!("{count} questions are waiting for your answers"),
    };
    match &open.first_title {
        Some(title) => format!("{waiting}: {title}"),
        None => waiting,
    }
}

fn notify(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    open: &OpenTurnQuestions,
    raise_attention: bool,
) {
    let body = crate::notification_feed::store::bounded_notification_body(&notification_body(open));
    if raise_attention {
        // The turn ended without attention, so nothing sounded yet: raise it once, the way any
        // other wait for input is raised.
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("activity".to_string(), json!("attention"));
        let endpoint = "/api/updateAgentActivity";
        let Ok(output) = crate::agents::dispatch_agent_endpoint(
            repository,
            db,
            &state.paths.home_dir,
            endpoint,
            &params,
            None,
        ) else {
            return;
        };
        crate::server::log_agent_activity_transition(state, endpoint, &params, &output.result);
        crate::notification_feed::observe_agent_activity_result(
            state,
            endpoint,
            &params,
            &output.result,
        );
        let _ = crate::server::schedule_presentation_session_delta(
            state, db, repository, project_id, session_id,
        );
    }
    match crate::notification_feed::store::mark_unread_session_notification_needs_input(
        db, session_id, &body,
    ) {
        Ok(changed) if changed > 0 => {
            crate::notification_feed::broadcast_notification_feed_changed(state)
        }
        Ok(_) => {}
        Err(error) => {
            let _ = state.logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "notificationFeed.openQuestionsFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.message),
                details: Some(json!({ "sessionId": session_id })),
            });
        }
    }
}
