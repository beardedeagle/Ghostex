use std::path::PathBuf;

use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};

use crate::server::RoutedResponse;

use super::{
    prepare::PAGE_CONTENT_SECURITY_POLICY,
    store::{is_page_id, read_page, read_record},
    theme::PAGE_STATE_MARKER,
};

pub(crate) const VISUAL_PAGE_ROUTE_PREFIX: &str = "/visual/";

/// `GET /visual/<id>`: a page `ghostex show` published, answered before the auth gate.
///
/// CDXC:ServerApi 2026-10-06 WHY:
/// The browser page that opens a chat card has no gxserver auth token, so the unguessable 128-bit id in the path is the only grant, as the `/open-file/` tokens are. The `sandbox` policy in the response header gives the page an opaque origin: its scripts run, but they cannot read gxserver's cookies or storage or call it as a same-origin page, and the content policy beside it keeps every fetch on https:.
///
/// CDXC:SessionChat 2026-10-06 DECISION:
/// User: no JavaScript engine is added to the desktop. An HTML page runs only in a browser page (Ghostex's browser, the system browser, or the phone's WebView) that opens this URL, never inline in the GPUI chat.
pub(crate) async fn serve(
    gxserver_state_dir: PathBuf,
    path: String,
    host: Option<String>,
) -> RoutedResponse {
    let Some(id) = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .filter(|id| is_page_id(id))
        .map(str::to_string)
    else {
        return status(StatusCode::NOT_FOUND);
    };
    let page_id = id.clone();
    let page = tokio::task::spawn_blocking(move || {
        read_page(&gxserver_state_dir, &id)
            .map(|page| with_page_state(page, read_record(&gxserver_state_dir, &id)))
    })
    .await
    .ok()
    .flatten();
    let Some(page) = page else {
        return status(StatusCode::NOT_FOUND);
    };
    let mut response = RoutedResponse {
        endpoint_path: None,
        response: Response::new(Body::from(page)),
    };
    let content_security_policy = format!(
        "sandbox allow-scripts allow-forms allow-popups; {}",
        header_policy(&page_id, host.as_deref())
    );
    for (name, value) in [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (
            header::CONTENT_SECURITY_POLICY,
            content_security_policy.as_str(),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CACHE_CONTROL, "no-store"),
    ] {
        if let Ok(value) = HeaderValue::from_str(value) {
            response.response.headers_mut().insert(name, value);
        }
    }
    response
}

fn status(code: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: code.into_response(),
    }
}

/// The page's content policy for this response. The page's own copy allows it to reach its server
/// (`'self'`), and a browser enforces both policies, so naming the page's own routes here (its
/// answers, its draft and its scroll position, under `/visual/<id>/`) keeps the page off every
/// other gxserver route.
fn header_policy(id: &str, host: Option<&str>) -> String {
    let host = host.filter(|host| {
        !host.is_empty()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-:[]".contains(&byte))
    });
    match host {
        Some(host) => PAGE_CONTENT_SECURITY_POLICY.replace(
            "connect-src https: 'self'",
            &format!("connect-src https: http://{host}{VISUAL_PAGE_ROUTE_PREFIX}{id}/"),
        ),
        None => {
            PAGE_CONTENT_SECURITY_POLICY.replace("connect-src https: 'self'", "connect-src https:")
        }
    }
}

/// The page with its answered state where `PAGE_API` reads it: whether the reader already sent
/// answers, what they were, and when, or else the draft they left. A page published before
/// answering existed has no marker.
fn with_page_state(page: Vec<u8>, record: Option<serde_json::Value>) -> Vec<u8> {
    let html = match String::from_utf8(page) {
        Ok(html) => html,
        Err(error) => return error.into_bytes(),
    };
    let Some(at) = html.find(PAGE_STATE_MARKER) else {
        return html.into_bytes();
    };
    let answered_at = record
        .as_ref()
        .and_then(|record| record.get("answeredAt"))
        .and_then(serde_json::Value::as_str);
    let state = match answered_at {
        Some(answered_at) => serde_json::json!({
            "answered": true,
            "answeredAt": answered_at,
            "answers": record.as_ref().and_then(|record| record.get("answers")).cloned(),
        }),
        None => serde_json::json!({
            "answered": false,
            "draft": record.as_ref().and_then(|record| record.get("draft")).cloned(),
        }),
    };
    // `<` never appears raw inside the script, so no value can close it.
    let state = state.to_string().replace('<', "\\u003c");
    let mut out = String::with_capacity(html.len() + state.len() + 64);
    out.push_str(&html[..at]);
    out.push_str("<script>window.ghostex&&ghostex.__state(");
    out.push_str(&state);
    out.push_str(");</script>");
    out.push_str(&html[at + PAGE_STATE_MARKER.len()..]);
    out.into_bytes()
}
