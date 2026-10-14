//! `/api/renameCoordinatorThread`: an orchestrator renames one of its threads.

use serde_json::{json, Map, Value};

use super::records::read_thread;
use crate::domain::{DomainRepository, DomainStateError};

/// The longest thread title an orchestrator may give.
const THREAD_TITLE_MAX_CHARS: usize = 120;

/// CDXC:Coordinators 2026-10-14 DECISION:
/// User: "we need to rename the sub sessions better i think no? ex this one: Accounts page redesign mockups is not about mockups since a long time now". They picked: threads are named after the area they own, the orchestrator renames a thread when a follow-up moves it to another area (`ghostex orchestrator rename`), and the user's own renames always win. The title is written straight to the session as the orchestrator's (`titleSource: "orchestrator"`), never typed into the agent as `/rename`, so the agent's own naming stays out of it and `keeps_its_given_title` keeps it. A thread the user renamed (a rename request from the sidebar or `ghostex rename-session` leaves `pendingAgentTitleRequestTitleSource: "user"`) is refused and left alone.
/// SEE-ALSO: server/src/coordinators/title.rs (`keeps_its_given_title`), server/src/agents/fork.rs (`request_session_rename`, where a user rename is recorded), server/src/ghostex_cli/coordinator/threads.rs (`rename`), server/src/coordinators/role.md.
pub fn rename_thread(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<(Value, (String, String)), DomainStateError> {
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
            "Renaming a thread needs projectId and sessionId.",
        ));
    };
    let (Some(coordinator_project_id), Some(coordinator_session_id)) =
        (text("coordinatorProjectId"), text("coordinatorSessionId"))
    else {
        return Err(DomainStateError::bad_request(
            "Renaming a thread needs the orchestrator's coordinatorProjectId and coordinatorSessionId.",
        ));
    };
    let title = text("title")
        .filter(|title| !title.chars().any(char::is_control))
        .filter(|title| title.chars().count() <= THREAD_TITLE_MAX_CHARS)
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "Give the thread a one-line title of 1 to {THREAD_TITLE_MAX_CHARS} characters."
            ))
        })?;
    let thread = read_thread(db, &project_id, &session_id)?
        .filter(|thread| {
            thread.coordinator_project_id == coordinator_project_id
                && thread.coordinator_session_id == coordinator_session_id
        })
        .ok_or_else(|| {
            DomainStateError::not_found(
                "That session is not a thread of this orchestrator, so it was not renamed.",
            )
        })?;
    let session = repository
        .get_session(&thread.project_id, &thread.session_id)?
        .ok_or_else(|| DomainStateError::not_found("The thread's session no longer exists."))?;
    let current = session["title"].as_str().unwrap_or_default().to_string();
    if session
        .pointer("/runtimeSettings/pendingAgentTitleRequestTitleSource")
        .and_then(Value::as_str)
        == Some("user")
    {
        return Err(DomainStateError {
            code: "userRenamed",
            message: format!(
                "The user renamed this thread to \"{current}\" themselves, so it keeps that name. Ask the user if it should change."
            ),
        });
    }
    let key = (thread.project_id.clone(), thread.session_id.clone());
    if current == title {
        return Ok((
            json!({ "changed": false, "title": title, "previousTitle": current }),
            key,
        ));
    }
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.insert("titleSource".into(), json!("orchestrator"));
    repository.update_session(
        json!({
            "projectId": thread.project_id,
            "sessionId": thread.session_id,
            "title": title,
            "runtimeSettings": runtime,
        })
        .as_object()
        .expect("object literal"),
    )?;
    Ok((
        json!({ "changed": true, "title": title, "previousTitle": current }),
        key,
    ))
}
