//! Requests to a workspace's running Executor over its local API, as a local program: the
//! workspace's bearer key, `127.0.0.1` as the host and no browser headers.

use std::time::Duration;

use serde_json::Value;

/// Where a workspace's Executor answers, and the key it accepts.
#[derive(Clone)]
pub(super) struct Endpoint {
    pub(super) port: u16,
    pub(super) api_key: String,
}

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const SLOW_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(timeout)
        .redirects(0)
        .user_agent("Ghostex-Connectors/1.0")
        .build()
}

/// Executor's error text: its `message`, else its `_tag`.
fn error_text(status: u16, body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            parsed
                .as_ref()
                .and_then(|value| value.get("_tag"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    match message {
        Some(message) => message,
        None => format!("Executor answered HTTP {status}."),
    }
}

fn send(
    endpoint: &Endpoint,
    method: &str,
    path: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Value, String> {
    let url = format!("http://127.0.0.1:{}{path}", endpoint.port);
    let request = agent(timeout)
        .request(method, &url)
        .set("authorization", &format!("Bearer {}", endpoint.api_key));
    let response = match body {
        Some(body) => request.send_json(body.clone()),
        None => request.call(),
    };
    match response {
        Ok(response) => {
            let text = response
                .into_string()
                .map_err(|error| format!("Couldn't read Executor's answer: {error}"))?;
            if text.trim().is_empty() {
                return Ok(Value::Null);
            }
            serde_json::from_str(&text)
                .map_err(|error| format!("Executor's answer wasn't JSON: {error}"))
        }
        Err(ureq::Error::Status(status, response)) => Err(error_text(
            status,
            &response.into_string().unwrap_or_default(),
        )),
        Err(ureq::Error::Transport(transport)) => Err(format!(
            "This workspace's Executor isn't answering ({transport})."
        )),
    }
}

pub(super) fn request(
    endpoint: &Endpoint,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    send(endpoint, method, path, body, DEFAULT_TIMEOUT)
}

/// For app imports, which build the app before answering.
pub(super) fn request_slow(
    endpoint: &Endpoint,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    send(endpoint, method, path, body, SLOW_TIMEOUT)
}

/// True when Executor answers its API with this key.
pub(super) fn ready(endpoint: &Endpoint) -> bool {
    send(endpoint, "GET", "/v1/apps", None, Duration::from_secs(3)).is_ok()
}
