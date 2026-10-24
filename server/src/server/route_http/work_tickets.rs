//! Work mode ticket routes (crate::work_mode): start work on a Linear or GitHub ticket, create a
//! Linear ticket, and the teams and Linear projects the Create Linear ticket dialog offers.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{
    create_linear_issue, default_linear_team_id, list_linear_projects, list_linear_teams,
    project_linear_api_key, project_linear_identifiers, resolve_work_mode_project,
    start_work_on_ticket, NewLinearTicket,
};

use super::super::work_mode_sync::spawn_work_mode_refresh;
use super::*;

pub(super) async fn route_work_tickets_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/startWorkOnTicket" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
            let result = start_work_on_ticket(&state, &params).await;
            spawn_work_mode_refresh(&state);
            match result {
                Ok(result) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(error) => {
                    project_worktree_operation_error_response(endpoint.path, request_id, error)
                }
            }
        }
        // Off the async runtime: each of these is a network call to Linear.
        "/api/createLinearIssue" | "/api/listLinearTeams" | "/api/listLinearProjects" => {
            let worker_state = state.clone();
            let worker_endpoint = endpoint.path.clone();
            let worker_request_id = request_id.clone();
            let response = tokio::task::spawn_blocking(move || {
                let path = worker_endpoint.clone();
                handle_domain_http(
                    &worker_state,
                    worker_endpoint,
                    worker_request_id,
                    &body_json,
                    |_, _, params, _| match path.as_str() {
                        "/api/createLinearIssue" => create_linear_ticket(&worker_state, params),
                        "/api/listLinearTeams" => linear_teams(&worker_state, params),
                        _ => linear_projects(&worker_state, params),
                    },
                )
            })
            .await;
            match response {
                Ok(response) => response,
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!("Linear request failed: {error}")),
                ),
            }
        }
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
            })
        }
    })
}

fn project_linear_key(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(String, String), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let project = resolve_work_mode_project(
        &DomainRepository::new(&db, state.metadata.server_id.as_str()),
        params,
    )?;
    let project_id = value_text(&project, "projectId")?;
    let api_key = project_linear_api_key(state, &project).ok_or_else(|| {
        DomainStateError::bad_request(
            "Set a Linear API key first (Settings, or ghostex issues linear-key).",
        )
    })?;
    Ok((project_id, api_key))
}

/// `{ teams: [{ id, key, name }], defaultTeamId }`.
fn linear_teams(state: &AppState, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let (project_id, api_key) = project_linear_key(state, params)?;
    let teams = list_linear_teams(&api_key).map_err(DomainStateError::bad_request)?;
    let used = project_linear_identifiers(state, &project_id)?;
    Ok(json!({
        "teams": teams,
        "defaultTeamId": default_linear_team_id(&teams, &used),
    }))
}

/// `{ projects: [{ id, name }] }`: open Linear projects, of one team when `teamId` is given.
fn linear_projects(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (_, api_key) = project_linear_key(state, params)?;
    let team_id = text(params, "teamId");
    let projects = list_linear_projects(&api_key, team_id.as_deref())
        .map_err(DomainStateError::bad_request)?;
    Ok(json!({ "projects": projects }))
}

/// `{ identifier, url, branchName }`. With no `teamId` the ticket goes to the team this
/// project's sessions use most.
fn create_linear_ticket(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (project_id, api_key) = project_linear_key(state, params)?;
    let title = text(params, "title")
        .ok_or_else(|| DomainStateError::bad_request("A ticket needs a title."))?;
    let team_id = match text(params, "teamId") {
        Some(team_id) => team_id,
        None => {
            let teams = list_linear_teams(&api_key).map_err(DomainStateError::bad_request)?;
            let used = project_linear_identifiers(state, &project_id)?;
            default_linear_team_id(&teams, &used).ok_or_else(|| {
                DomainStateError::bad_request("This Linear workspace has no teams.")
            })?
        }
    };
    let description = text(params, "description");
    let linear_project_id = text(params, "linearProjectId");
    let ticket = create_linear_issue(
        &api_key,
        &NewLinearTicket {
            team_id: &team_id,
            title: &title,
            description: description.as_deref(),
            linear_project_id: linear_project_id.as_deref(),
            assign_to_me: params.get("assignToMe").and_then(Value::as_bool) != Some(false),
        },
    )
    .map_err(DomainStateError::bad_request)?;
    Ok(json!({
        "identifier": ticket.identifier,
        "url": ticket.url,
        "branchName": ticket.branch_name,
    }))
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
