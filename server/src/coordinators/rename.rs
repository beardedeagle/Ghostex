//! `/api/renameCoordinatorThread`: a thread renames itself (`ghostex orchestrator rename-self`),
//! or its orchestrator renames it (`ghostex orchestrator rename`).

use serde_json::{json, Map, Value};

use super::records::read_thread;
use crate::domain::{DomainRepository, DomainStateError};

/// The longest thread title an orchestrator may give.
const THREAD_TITLE_MAX_CHARS: usize = 120;

/// CDXC:Coordinators 2026-10-14 DECISION:
/// User: "we need to rename the sub sessions better i think no? ex this one: Accounts page redesign mockups is not about mockups since a long time now", and picked "Let each thread rename itself when it starts a new task"; the user's own renames always win. A thread is named after the area of its current task: the brief tells it so (brief.rs), and when its orchestrator's message starts a task in another area it renames itself with `ghostex orchestrator rename-self` (`selfRename`, `titleSource: "thread"`). The orchestrator's own `ghostex orchestrator rename` stays as a quiet tool (`titleSource: "orchestrator"`), which the user approved for renaming the threads that already exist. Either way the title is written straight to the session, never typed into the agent as `/rename`, so the agent's auto-naming stays out of it and `keeps_its_given_title` keeps it. A thread the user renamed (a rename request from the sidebar or `ghostex rename-session` leaves `pendingAgentTitleRequestTitleSource: "user"`) is refused and keeps its name.
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
    let self_rename = params.get("selfRename").and_then(Value::as_bool) == Some(true);
    let coordinator = (text("coordinatorProjectId"), text("coordinatorSessionId"));
    if !self_rename && (coordinator.0.is_none() || coordinator.1.is_none()) {
        return Err(DomainStateError::bad_request(
            "Renaming a thread needs the orchestrator's coordinatorProjectId and coordinatorSessionId.",
        ));
    }
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
            self_rename
                || (coordinator.0.as_deref() == Some(thread.coordinator_project_id.as_str())
                    && coordinator.1.as_deref() == Some(thread.coordinator_session_id.as_str()))
        })
        .ok_or_else(|| {
            DomainStateError::not_found(if self_rename {
                "This session is not an orchestrator's thread, so rename-self does not apply. Only a thread renames itself this way."
            } else {
                "That session is not a thread of this orchestrator, so it was not renamed."
            })
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
            message: if self_rename {
                format!(
                    "The user renamed you to \"{current}\" themselves, so keep that name and do not rename yourself."
                )
            } else {
                format!(
                    "The user renamed this thread to \"{current}\" themselves, so it keeps that name. Ask the user if it should change."
                )
            },
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
    runtime.insert(
        "titleSource".into(),
        json!(if self_rename {
            "thread"
        } else {
            "orchestrator"
        }),
    );
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
