//! The session an agent's connector tools serve, and the workspace whose connectors it gets.

use serde_json::{Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

use super::super::workspace::project_workspace;

pub(crate) struct AgentSession {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) workspace: String,
}

fn param<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Connector tools only work inside a Ghostex session (no session was named).",
            )
        })
}

/// The session `projectId` and `sessionId` name. Blocking.
pub(crate) fn resolve(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<AgentSession, DomainStateError> {
    let project_id = param(params, "projectId")?;
    let session_id = param(params, "sessionId")?;
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        DomainStateError::corrupt_state(format!("SQLite gxserver state error: {error}"))
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::not_found("This session no longer exists."))?;
    let project = repository
        .get_project(project_id)?
        .ok_or_else(|| DomainStateError::not_found("This session's project no longer exists."))?;
    Ok(AgentSession {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        workspace: project_workspace(&db, &repository, &project)?,
    })
}
