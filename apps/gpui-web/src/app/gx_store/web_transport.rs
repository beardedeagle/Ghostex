//! The browser's gxserver transport: `fetch` for RPC and one `WebSocket` for the event stream, in place of the native client's `ureq` and `tungstenite` thread (`packages/gx-client`). Everything arrives on the page's one thread, so events go into a channel the GPUI task drains.
use futures::channel::mpsc::UnboundedSender;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{MessageEvent, Request, RequestInit, Response, WebSocket};

const PROTOCOL_VERSION: u64 = 1;

/// Where the daemon is and the token that opens it, as `ghostex web` hands them out.
#[derive(Clone, Debug)]
pub(crate) struct GxserverEndpoint {
    pub(crate) base_url: String,
    pub(crate) auth_token: String,
}

thread_local! {
    static CURRENT_ENDPOINT: std::cell::RefCell<Option<GxserverEndpoint>> = const { std::cell::RefCell::new(None) };
}

/// The daemon the page is connected to, for the shared files that make their own calls.
pub(crate) fn current_endpoint() -> Option<GxserverEndpoint> {
    CURRENT_ENDPOINT.with(|endpoint| endpoint.borrow().clone())
}

pub(crate) enum StreamEvent {
    Open,
    Frame(String),
    Closed,
}

fn js_error(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            js_sys::JSON::stringify(&error)
                .ok()
                .and_then(|s| s.as_string())
        })
        .unwrap_or_else(|| "unknown JavaScript error".to_string())
}

async fn post_envelope(url: &str, token: Option<&str>, body: &Value) -> Result<Value, String> {
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(&body.to_string()));
    let request = Request::new_with_str_and_init(url, &init).map_err(js_error)?;
    let headers = request.headers();
    headers
        .set("content-type", "application/json")
        .map_err(js_error)?;
    headers
        .set("x-gxserver-protocol-version", &PROTOCOL_VERSION.to_string())
        .map_err(js_error)?;
    if let Some(token) = token {
        headers
            .set("authorization", &format!("Bearer {token}"))
            .map_err(js_error)?;
    }
    let window = web_sys::window().ok_or("no window")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let text = JsFuture::from(response.text().map_err(js_error)?)
        .await
        .map_err(js_error)?
        .as_string()
        .unwrap_or_default();
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

async fn post_json(url: &str, token: Option<&str>, body: &Value) -> Result<Value, String> {
    let envelope = post_envelope(url, token, body).await?;
    if envelope["ok"] == true {
        Ok(envelope["result"].clone())
    } else {
        Err(envelope["message"]
            .as_str()
            .or_else(|| envelope["error"].as_str())
            .unwrap_or("request failed")
            .to_string())
    }
}

/// Asks the page's own server (`ghostex web`, or the dev server proxying to it) for the daemon.
pub(crate) async fn bootstrap() -> Result<GxserverEndpoint, String> {
    let result = post_json("/api/webBootstrap", None, &json!({})).await?;
    let endpoint = GxserverEndpoint {
        base_url: result["baseUrl"]
            .as_str()
            .ok_or("bootstrap has no baseUrl")?
            .to_string(),
        auth_token: result["authToken"]
            .as_str()
            .ok_or("bootstrap has no authToken")?
            .to_string(),
    };
    CURRENT_ENDPOINT.with(|current| *current.borrow_mut() = Some(endpoint.clone()));
    // The built-in features on for this computer; an older `ghostex web` sends none (all off).
    crate::app::read_aloud::set_computer_features(
        result["builtInFeatures"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str().map(str::to_string))
            .collect(),
    );
    Ok(endpoint)
}

/// One gxserver RPC: `POST {base}{path}` with the `{params, protocolVersion}` envelope.
pub(crate) async fn rpc(
    endpoint: &GxserverEndpoint,
    path: &str,
    params: Value,
) -> Result<Value, String> {
    post_json(
        &format!("{}{path}", endpoint.base_url),
        Some(&endpoint.auth_token),
        &json!({"params": params, "protocolVersion": PROTOCOL_VERSION}),
    )
    .await
}

/// One gxserver RPC for callers that need the daemon's error CODE as well as its message (the chat composer branches on `composerNotReady` and `sendCancelled`).
pub(crate) async fn rpc_envelope(
    endpoint: &GxserverEndpoint,
    path: &str,
    params: Value,
) -> Result<Value, Value> {
    let fail = |message: String| json!({"message": message, "endpoint": path});
    let body = json!({"params": params, "protocolVersion": PROTOCOL_VERSION});
    let mut envelope = post_envelope(
        &format!("{}{path}", endpoint.base_url),
        Some(&endpoint.auth_token),
        &body,
    )
    .await
    .map_err(fail)?;
    if envelope["ok"] != true {
        return Err(
            json!({"code": envelope["error"], "message": envelope["message"], "endpoint": path}),
        );
    }
    Ok(envelope["result"].take())
}

/// Opens `/api/events`. Browsers cannot set headers on a WebSocket, so the token rides the query string. The returned socket must be kept alive by the caller.
pub(crate) fn open_events(
    endpoint: &GxserverEndpoint,
    events: UnboundedSender<StreamEvent>,
) -> Result<WebSocket, String> {
    let base = endpoint.base_url.replacen("http", "ws", 1);
    let url = format!(
        "{base}/api/events?authToken={}&protocolVersion={PROTOCOL_VERSION}",
        js_sys::encode_uri_component(&endpoint.auth_token)
    );
    let socket = WebSocket::new(&url).map_err(js_error)?;

    let sender = events.clone();
    let on_open = Closure::<dyn FnMut()>::new(move || {
        let _ = sender.unbounded_send(StreamEvent::Open);
    });
    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    on_open.forget();

    let sender = events.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string() {
            let _ = sender.unbounded_send(StreamEvent::Frame(text));
        }
    });
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    let on_close = Closure::<dyn FnMut()>::new(move || {
        let _ = events.unbounded_send(StreamEvent::Closed);
    });
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    on_close.forget();

    Ok(socket)
}
