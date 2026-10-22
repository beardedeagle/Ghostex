//! The answer a gxserver gives while it holds its port but is still starting, and how long a
//! client waits before sending the request again.
//!
//! CDXC:ServerDaemon 2026-10-22 WHY:
//! A request that reached a starting gxserver used to sit in the listen backlog until startup finished; since gxserver answers HTTP 503 `serverStarting` instead (the DECISION in server/src/server/startup_gate.rs), a client that used to wait would fail at once, so a hook event fired during a restart was lost. Every client that used to wait sends the request again after the advised delay, within the time limit it already had: nothing ran behind that answer, so a resend cannot run the request twice. This is the one rule they share; each transport keeps its own loop.
//! SEE-ALSO: `wait_out_server_starting` in server/src/http_client.rs (the CLI, hooks and `gxserver` lifecycle calls), `gxserver_post_typed_operation_across_restart` in apps/desktop/src/app/helpers/board_gxserver/typed_operations.rs (the desktop, local and remote), `rpc_envelope` in apps/gpui-web/src/app/gx_store/web_transport.rs (the web build), `attempt` in apps/mobile/app/src/chat/rust/machine-link.ts (the phone, which repeats this rule in TypeScript).

use std::time::Duration;

use serde_json::Value;

use crate::GXSERVER_PRODUCT;

/// The `error` code of that answer.
pub const SERVER_STARTING_ERROR: &str = "serverStarting";
/// The delay the daemon advises in the answer's `retryAfterMs`. Its `Retry-After` header carries
/// the same advice in whole seconds, the smallest unit that header has.
pub const SERVER_STARTING_RETRY_AFTER_MS: u64 = 250;

/// How long to wait before sending the request again when `body` is a starting gxserver's answer;
/// `None` for any other answer.
pub fn server_starting_retry_after_value(body: &Value) -> Option<Duration> {
    if body.get("product").and_then(Value::as_str) != Some(GXSERVER_PRODUCT)
        || body.get("error").and_then(Value::as_str) != Some(SERVER_STARTING_ERROR)
    {
        return None;
    }
    let advised = body
        .get("retryAfterMs")
        .and_then(Value::as_u64)
        .unwrap_or(SERVER_STARTING_RETRY_AFTER_MS);
    Some(Duration::from_millis(advised.clamp(50, 2_000)))
}

/// [`server_starting_retry_after_value`] for a raw HTTP answer: only an HTTP 503 can be one.
pub fn server_starting_retry_after(status: u16, body: &str) -> Option<Duration> {
    if status != 503 {
        return None;
    }
    server_starting_retry_after_value(&serde_json::from_str(body.trim()).ok()?)
}
