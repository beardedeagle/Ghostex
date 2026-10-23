use super::*;

use crate::agent_prompt_search::PromptLaunchSession;

/*
CDXC:PromptSearch 2026-08-20:
The Find surface's four RPCs. All of them go through the one warm
`ghostex_find::SearchIndex` in `agent_prompt_search`, which is the same index, favorites
file, and ranking `gx f` uses, so the GUI and the terminal picker can never
disagree about what matched or what is starred.
*/
pub(crate) fn agent_prompt_search_params(
    body: &Value,
) -> Result<Map<String, Value>, crate::agent_prompt_search::PromptSearchError> {
    match read_domain_rpc_params(body) {
        Ok(params) => Ok(params),
        Err(error) => Err(crate::agent_prompt_search::PromptSearchError {
            code: error.code,
            message: error.message,
        }),
    }
}

pub(crate) fn agent_prompt_search_response(
    endpoint_path: String,
    request_id: String,
    outcome: Result<Value, crate::agent_prompt_search::PromptSearchError>,
) -> RoutedResponse {
    match outcome {
        Ok(payload) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, payload),
        ),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: error.code,
                message: error.message,
            },
        ),
    }
}

pub(crate) fn handle_search_agent_prompts_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body).and_then(|params| {
        let sessions = read_open_sessions_for_prompt_search(state)?;
        crate::agent_prompt_search::search_agent_prompts(&state.paths, &params, &sessions)
    });
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

pub(crate) fn handle_read_agent_prompt_text_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body).and_then(|params| {
        crate::agent_prompt_search::read_agent_prompt_text(&state.paths, &params)
    });
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

pub(crate) fn handle_toggle_agent_prompt_favorite_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body).and_then(|params| {
        crate::agent_prompt_search::toggle_agent_prompt_favorite(&state.paths, &params)
    });
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

pub(crate) fn handle_resolve_agent_prompt_launch_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body).and_then(|params| {
        let accept_all_default = read_agent_accept_all_enabled_for_prompt_launch(state);
        crate::agent_prompt_search::resolve_agent_prompt_launch(
            &state.paths,
            &params,
            |agent_session_id| read_sessions_owning_agent_conversation(state, agent_session_id),
            accept_all_default,
        )
    });
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

/// The daemon's Accept All policy, the same value `gx f` reads before handing
/// zehn `--accept-all`. A read failure means the policy is unknown, and an
/// unknown permission policy must not silently become "bypass permissions".
pub(crate) fn read_agent_accept_all_enabled_for_prompt_launch(state: &AppState) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    crate::agents::read_agent_settings(&db)
        .ok()
        .and_then(|settings| {
            settings
                .get("agentAcceptAllEnabled")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

/// Session rows that are not stopped, each paired with the agent family its launch configuration resumes with.
/// A search runs per keystroke and only names rows, so it skips the stopped history.
pub(crate) fn read_open_sessions_for_prompt_search(
    state: &AppState,
) -> Result<Vec<PromptLaunchSession>, crate::agent_prompt_search::PromptSearchError> {
    let db = open_prompt_launch_database(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository
        .list_projects()
        .map_err(prompt_launch_domain_error)?
        .into_iter()
        .filter_map(|project| {
            let project_id = project
                .get("projectId")
                .and_then(Value::as_str)?
                .to_string();
            Some((project_id, project))
        })
        .collect::<HashMap<String, Value>>();
    let sessions = repository
        .list_sessions_excluding_stopped(None)
        .map_err(prompt_launch_domain_error)?;
    Ok(sessions
        .into_iter()
        .map(|session| {
            let project = session
                .get("projectId")
                .and_then(Value::as_str)
                .and_then(|project_id| projects.get(project_id))
                .cloned()
                .unwrap_or(Value::Null);
            prompt_launch_session(&project, session)
        })
        .collect())
}

/// Every session row recorded against one agent conversation, so the resolver can decide whether an open Ghostex session already owns it.
/// Stopped rows are included, because a stopped row whose provider still exists owns its conversation too.
pub(crate) fn read_sessions_owning_agent_conversation(
    state: &AppState,
    agent_session_id: &str,
) -> Result<Vec<PromptLaunchSession>, crate::agent_prompt_search::PromptSearchError> {
    let db = open_prompt_launch_database(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let sessions = repository
        .list_sessions_with_agent_session_id(agent_session_id)
        .map_err(prompt_launch_domain_error)?;
    sessions
        .into_iter()
        .map(|session| {
            let project = match session.get("projectId").and_then(Value::as_str) {
                Some(project_id) => repository
                    .get_project(project_id)
                    .map_err(prompt_launch_domain_error)?
                    .unwrap_or(Value::Null),
                None => Value::Null,
            };
            Ok(prompt_launch_session(&project, session))
        })
        .collect()
}

fn open_prompt_launch_database(
    state: &AppState,
) -> Result<crate::storage::GxserverDatabase, crate::agent_prompt_search::PromptSearchError> {
    open_gxserver_database(&state.paths).map_err(|error| {
        crate::agent_prompt_search::PromptSearchError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        }
    })
}

fn prompt_launch_domain_error(
    error: DomainStateError,
) -> crate::agent_prompt_search::PromptSearchError {
    crate::agent_prompt_search::PromptSearchError {
        code: error.code,
        message: error.message,
    }
}

fn prompt_launch_session(project: &Value, session: Value) -> PromptLaunchSession {
    let agent_family_id = crate::agents::session_agent_family_id(project, &session);
    let named_title = prompt_launch_session_named_title(project, &session);
    PromptLaunchSession {
        session,
        agent_family_id,
        named_title,
    }
}

/// The session's own title, or `None` while it is still the agent's placeholder
/// ("Claude Session"), which says less than the title in the agent transcript.
fn prompt_launch_session_named_title(project: &Value, session: &Value) -> Option<String> {
    let title = session.get("title").and_then(Value::as_str)?.trim();
    let placeholder = crate::agents::project_agent_session_default_title(project, session);
    let bare = |text: &str| text.trim_start_matches('∗').trim().to_lowercase();
    if title.is_empty() || bare(title) == bare(&placeholder) {
        return None;
    }
    Some(title.to_string())
}
