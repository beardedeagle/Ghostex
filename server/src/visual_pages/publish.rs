use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::{
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{clip_chars, write_page, VISUAL_PAGE_MAX_BYTES, VISUAL_PAGE_TITLE_MAX_CHARS},
};

pub(crate) const PUBLISH_VISUAL_PAGE_ENDPOINT: &str = "/api/publishVisualPage";
/// The JSON body carries the page as an escaped string, which can grow it to twice its size.
pub(crate) const PUBLISH_VISUAL_PAGE_BODY_LIMIT_BYTES: usize =
    2 * VISUAL_PAGE_MAX_BYTES + 64 * 1024;

/// `/api/publishVisualPage` `{title, html, sessionRef?, sourcePath?}`: stores a page `ghostex show`
/// prepared and answers `{id, title, path}`, where `path` is the `/visual/<id>` route that serves
/// it. `sourcePath` is the HTML file it was prepared from, kept for Refresh (`source.rs`).
pub(crate) fn handle_publish_visual_page_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let Some(html) = params
        .get("html")
        .and_then(Value::as_str)
        .filter(|html| !html.trim().is_empty())
    else {
        return bad_request(
            endpoint_path,
            request_id,
            "publishVisualPage requires html.",
        );
    };
    if html.len() > VISUAL_PAGE_MAX_BYTES {
        return bad_request(
            endpoint_path,
            request_id,
            "The page is larger than the 8 MiB limit.",
        );
    }
    let Some(title) = text("title").map(|title| clip_chars(title, VISUAL_PAGE_TITLE_MAX_CHARS))
    else {
        return bad_request(
            endpoint_path,
            request_id,
            "publishVisualPage requires title.",
        );
    };
    let source = text("sourcePath")
        .map(std::path::Path::new)
        .filter(|path| path.is_absolute() && path.is_file());
    let id = match write_page(
        &state.paths.root_dir,
        &title,
        html,
        text("sessionRef"),
        source,
    ) {
        Ok(id) => id,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("Could not save the page: {error}"),
                },
            )
        }
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "id": id,
                "title": title,
                "path": format!("{VISUAL_PAGE_ROUTE_PREFIX}{id}"),
            }),
        ),
    )
}

fn bad_request(endpoint_path: String, request_id: String, message: &str) -> RoutedResponse {
    domain_error_response(
        endpoint_path,
        request_id,
        DomainStateError::bad_request(message),
    )
}
