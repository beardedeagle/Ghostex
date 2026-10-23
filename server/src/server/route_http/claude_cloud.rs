//! `/api/claudeCloud`: Claude cloud sessions (crate::claude_cloud).

use crate::protocol::rpc_success;

use super::*;

pub(super) async fn route_claude_cloud_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if request.endpoint.path != "/api/claudeCloud" {
        return Err(request);
    }
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
    Ok(
        match crate::claude_cloud::claude_cloud_action(state, params).await {
            Ok(value) => routed_json(
                Some(endpoint.path),
                StatusCode::OK,
                rpc_success(request_id, value),
            ),
            Err(error) => domain_error_response(endpoint.path, request_id, error),
        },
    )
}
