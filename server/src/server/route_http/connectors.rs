//! Connectors routes (crate::connectors): read every workspace's connectors, sign a workspace in
//! to and out of Composio, list its tools again, connect one of its apps and call one of its tools;
//! turn a workspace's Executor on or off and manage its apps and tools; give agent sessions their
//! connector tools and run their calls; read and change a workspace's Ask/Allow/Never rules.

use serde_json::{Map, Value};

use crate::connectors::{
    add_executor_app, add_executor_mcp_server, broadcast_connectors_changed, call_composio_tool,
    call_connector_agent_tool, call_executor_tool, cancel_connector_agent_tool_call,
    connect_composio_app, connect_executor_app, connectors_enabled, disable_executor,
    enable_executor, list_composio_tools, list_connector_agent_tools, list_executor_tools,
    read_connector_rules, read_connectors, refresh_composio, remove_executor_app,
    search_executor_catalog, set_connector_rule, sign_in_composio, sign_out_composio,
    start_connectors_work, CONNECTORS_OFF_MESSAGE,
};
use crate::domain::DomainStateError;
use crate::protocol::rpc_success;

use super::*;

type ConnectorAction = fn(&AppState, &Map<String, Value>) -> Result<Value, DomainStateError>;

pub(super) async fn route_connectors_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let action: Option<ConnectorAction> = match request.endpoint.path.as_str() {
        "/api/readConnectors" => Some(read_connectors),
        "/api/signInComposio" => Some(sign_in_composio),
        "/api/signOutComposio" => Some(sign_out_composio),
        "/api/refreshComposio" => Some(refresh_composio),
        "/api/connectComposioApp" => Some(connect_composio_app),
        "/api/listComposioTools" => Some(list_composio_tools),
        "/api/callComposioTool" => Some(call_composio_tool),
        "/api/enableExecutor" => Some(enable_executor),
        "/api/disableExecutor" => Some(disable_executor),
        "/api/searchExecutorCatalog" => Some(search_executor_catalog),
        "/api/addExecutorApp" => Some(add_executor_app),
        "/api/addExecutorMcpServer" => Some(add_executor_mcp_server),
        "/api/removeExecutorApp" => Some(remove_executor_app),
        "/api/connectExecutorApp" => Some(connect_executor_app),
        "/api/listExecutorTools" => Some(list_executor_tools),
        "/api/callExecutorTool" => Some(call_executor_tool),
        "/api/listConnectorAgentTools" => Some(list_connector_agent_tools),
        "/api/cancelConnectorAgentToolCall" => Some(cancel_connector_agent_tool_call),
        "/api/readConnectorRules" => Some(read_connector_rules),
        "/api/setConnectorRule" => Some(set_connector_rule),
        // Answered on its own below: a call may wait on the user's card for 30 minutes.
        "/api/callConnectorAgentTool" => None,
        _ => return Err(request),
    };
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        ..
    } = request;
    if !connectors_enabled(&state.paths) {
        return Ok(domain_error_response(
            endpoint.path,
            request_id,
            DomainStateError::bad_request(CONNECTORS_OFF_MESSAGE),
        ));
    }
    start_connectors_work();
    let params = match read_domain_rpc_params(&body_json) {
        Ok(params) => params,
        Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
    };
    let Some(action) = action else {
        let result = call_connector_agent_tool(state.clone(), params).await;
        return Ok(routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ));
    };
    let signs_out = endpoint.path == "/api/signOutComposio";
    let worker_state = state.clone();
    // Off the async runtime: Composio is called over the network.
    let result = tokio::task::spawn_blocking(move || action(&worker_state, &params))
        .await
        .unwrap_or_else(|error| {
            Err(DomainStateError::corrupt_state(format!(
                "Connectors stopped unexpectedly: {error}"
            )))
        });
    if signs_out {
        if let Some(workspace) = result
            .as_ref()
            .ok()
            .and_then(|value| value.get("workspaceId"))
            .and_then(|value| value.as_str())
        {
            broadcast_connectors_changed(&state, workspace);
        }
    }
    Ok(match result {
        Ok(value) => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint.path, request_id, error),
    })
}
