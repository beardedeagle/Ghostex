//! Connectors routes (crate::connectors): read every workspace's connectors, sign a workspace in
//! to and out of Composio, list its tools again, connect one of its apps and call one of its tools.

use crate::connectors::{
    broadcast_connectors_changed, call_composio_tool, connect_composio_app, list_composio_tools,
    read_connectors, refresh_composio, sign_in_composio, sign_out_composio,
};
use crate::domain::DomainStateError;
use crate::protocol::rpc_success;

use super::*;

pub(super) async fn route_connectors_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let action = match request.endpoint.path.as_str() {
        "/api/readConnectors" => read_connectors,
        "/api/signInComposio" => sign_in_composio,
        "/api/signOutComposio" => sign_out_composio,
        "/api/refreshComposio" => refresh_composio,
        "/api/connectComposioApp" => connect_composio_app,
        "/api/listComposioTools" => list_composio_tools,
        "/api/callComposioTool" => call_composio_tool,
        _ => return Err(request),
    };
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        ..
    } = request;
    let params = match read_domain_rpc_params(&body_json) {
        Ok(params) => params,
        Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
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
