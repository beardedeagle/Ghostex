//! `/api/restorePreviousSession`: bring a closed session back as a new session that carries the
//! closed one's agent, conversation id, persistence name, tag, sidebar place and title, then drop
//! the closed history row. The agent itself comes back when a client attaches: the attach plan is
//! built from those fields, for every agent the same way.
//!
//! CDXC:Sessions 2026-10-14 DECISION:
//! User: "add ONE gxserver restore for closed sessions ... that restores every agent the way the desktop does today (not just Claude/Codex/Pi via /api/openConversation), and make the desktop, the 'ghostex restore-previous-session' CLI and the phone all use it, so there is one restore rule for every client. Move the desktop's restore logic ... into gxserver rather than duplicating it." This is the desktop's former `gpui_restore_previous_session_from_history_id` and its metadata and create-params helpers (apps/desktop/src/app/helpers/remote/previous_sessions.rs), unchanged except that a session with no closed row is refused instead of restored as an empty "Terminal Session".
//!
//! SEE-ALSO: server/src/server/previous_session_restore_http.rs, server/src/ghostex_cli/previous_sessions.rs, apps/mobile/app/src/sessions/closedSessions.ts.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

/// The title a restored session gets when its closed row has none.
const DEFAULT_TITLE: &str = "Terminal Session";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestorePreviousSessionRequest {
    pub project_id: String,
    pub session_id: String,
    /// Stamped on `/api/removeSession` for the closed row.
    pub reason: String,
}

impl RestorePreviousSessionRequest {
    pub fn read(params: &Map<String, Value>) -> Result<Self, DomainStateError> {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
            return Err(DomainStateError::bad_request(
                "restorePreviousSession needs projectId and sessionId.",
            ));
        };
        Ok(Self {
            project_id,
            session_id,
            reason: text("reason").unwrap_or_else(|| "restorePreviousSession".to_string()),
        })
    }
}

/// Restores the session. `call` performs one of this daemon's own endpoints and answers its
/// `result`. Answers `{ outcome: "restored", projectId, sessionId, zmxName? }` naming the new
/// session.
pub fn restore_previous_session(
    request: &RestorePreviousSessionRequest,
    call: &dyn Fn(&str, Value) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    let (project_id, session_id) = (request.project_id.as_str(), request.session_id.as_str());
    let previous = call(
        "/api/listPreviousSessions",
        json!({
            "includeActive": false,
            "includePrevious": true,
            "limit": 20,
            "projectId": project_id,
            "query": session_id,
        }),
    )?;
    let Some(row) = closed_row(&previous, project_id, session_id) else {
        return Err(DomainStateError::not_found(
            "This closed session is no longer in Previous Sessions.",
        ));
    };
    let created = call(
        "/api/createSession",
        create_params(project_id, session_id, row),
    )?;
    let session = created.get("session");
    let Some(restored_id) = non_empty(session, "sessionId") else {
        return Err(DomainStateError::bad_request(
            "The closed session could not be restored.",
        ));
    };
    let restored_project = non_empty(session, "projectId").unwrap_or(project_id.to_string());
    // A failure here leaves an extra history row and changes nothing else.
    let _ = call(
        "/api/removeSession",
        json!({
            "projectId": project_id,
            "reason": request.reason,
            "sessionId": session_id,
        }),
    );
    let mut answer =
        json!({ "outcome": "restored", "projectId": restored_project, "sessionId": restored_id });
    if let Some(zmx_name) = non_empty(session, "zmxName") {
        answer["zmxName"] = Value::String(zmx_name);
    }
    Ok(answer)
}

/// The closed row for the session; a row still marked running is taken only when no stopped one
/// matches.
fn closed_row<'a>(
    previous: &'a Value,
    project_id: &str,
    session_id: &str,
) -> Option<&'a Map<String, Value>> {
    let mut running_match = None;
    for row in previous
        .get("results")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_object)
    {
        if text(row, "projectId") != Some(project_id) || text(row, "sessionId") != Some(session_id)
        {
            continue;
        }
        let running = row.get("isRunning").and_then(Value::as_bool) == Some(true)
            || row.get("lifecycleState").and_then(Value::as_str) == Some("running")
            || row.get("providerSessionState").and_then(Value::as_str) == Some("running");
        if !running {
            return Some(row);
        }
        running_match.get_or_insert(row);
    }
    running_match
}

fn create_params(project_id: &str, session_id: &str, row: &Map<String, Value>) -> Value {
    let title = ["displayTitle", "primaryTitle", "title"]
        .iter()
        .find_map(|key| text(row, key))
        .unwrap_or(DEFAULT_TITLE);
    let mut create = Map::new();
    create.insert("kind".into(), json!("terminal"));
    if let Some(agent_id) = text(row, "agentId") {
        create.insert("agentId".into(), json!(agent_id));
    }
    create.insert("lifecycleState".into(), json!("running"));
    create.insert("projectId".into(), json!(project_id));
    create.insert("restoredFromSessionId".into(), json!(session_id));
    create.insert("surface".into(), json!("workspace"));
    create.insert("title".into(), json!(title));
    let runtime_settings: Map<String, Value> = [
        "agentSessionId",
        "sessionPersistenceName",
        "sessionPersistenceProvider",
    ]
    .iter()
    .filter_map(|key| text(row, key).map(|value| ((*key).to_string(), json!(value))))
    .collect();
    if !runtime_settings.is_empty() {
        create.insert("runtimeSettings".into(), Value::Object(runtime_settings));
    }
    if let Some(tag) = text(row, "sessionTag") {
        create.insert("sessionTag".into(), json!(tag));
    }
    if let Some(order) = row.get("sidebarOrder").filter(|order| order.is_number()) {
        create.insert("sidebarOrder".into(), order.clone());
    }
    Value::Object(create)
}

fn text<'a>(row: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn non_empty(value: Option<&Value>, key: &str) -> Option<String> {
    value?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}
