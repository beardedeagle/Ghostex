//! What the CLI sign-in routes do: sign a workspace in to `gh` or `gws` (the link to open, and
//! `gh`'s one-time code) or out of it. Blocking: the login is started and its link read first.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

use super::super::api::{failed, workspace_param};
use super::signin;
use super::tools::CliTool;

fn tool_param(params: &Map<String, Value>) -> Result<CliTool, DomainStateError> {
    params
        .get("tool")
        .and_then(Value::as_str)
        .and_then(CliTool::parse)
        .ok_or_else(|| failed("Name the tool: gh or gws.".to_string()))
}

/// `/api/signInConnectorCli`: starts the tool's login for the workspace (`tool`: `gh` or `gws`).
pub(crate) fn sign_in_connector_cli(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let tool = tool_param(params)?;
    let started = signin::sign_in(&state.paths, &workspace, tool).map_err(failed)?;
    let message = match &started.code {
        Some(code) => format!(
            "Enter the code {code} on the page that opens, then approve {}.",
            tool.name()
        ),
        None => format!("Approve {} on the page that opens.", tool.name()),
    };
    Ok(json!({
        "workspaceId": workspace,
        "tool": tool.id(),
        "url": started.url,
        "code": started.code,
        "message": message,
    }))
}

/// `/api/signOutConnectorCli`: the workspace's agents use the user's usual sign-in again.
pub(crate) fn sign_out_connector_cli(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let tool = tool_param(params)?;
    signin::sign_out(&state.paths, &workspace, tool);
    Ok(json!({ "workspaceId": workspace, "tool": tool.id(), "signedIn": false }))
}
