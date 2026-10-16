//! `ghostex connectors mcp`: the stdio MCP server every Claude and Codex session loads while
//! Connectors is on. It owns nothing: the tool list comes from gxserver
//! (`/api/listConnectorAgentTools`, the session's workspace's Composio and Executor tools) and
//! every call goes to `/api/callConnectorAgentTool` with the session it runs in, so gxserver can
//! gate it, show the approval card in that chat and run it.
//!
//! CDXC:Connectors 2026-10-16 WHY:
//! Claude Code and Codex speak newline-delimited JSON-RPC over stdio while other clients use Content-Length frames; the server answers in whichever framing the client used. Calls run on their own threads so a call waiting on the user's card (up to 30 minutes) never stops `ping`, `tools/list` or a cancellation from being answered. The tool list is read again every few seconds and `notifications/tools/list_changed` sent when it changed, so connecting an app or turning Executor on reaches sessions already running.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{json, Value};

use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::rpc::{
    request_gxserver_envelope, resolve_local_gxserver_target, CliError, CliResult, Target,
    GXSERVER_PROTOCOL_VERSION,
};

/// Longer than the 30 minutes a card may wait.
const TOOL_CALL_TIMEOUT_MS: u64 = 35 * 60 * 1000;
/// Listing may start the workspace's Executor once (its tools are kept after that).
const TOOL_LIST_TIMEOUT_MS: u64 = 120_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Framing {
    ContentLength,
    Lines,
}

struct Server {
    project_id: Option<String>,
    session_id: Option<String>,
    target: Target,
    framing: Mutex<Framing>,
    /// JSON-RPC request id → the call id gxserver knows, for `notifications/cancelled`.
    calls: Mutex<HashMap<String, String>>,
    /// A failed `tools/list` started a watcher that announces the tools once gxserver answers.
    watching_tools: AtomicBool,
    /// The names of the tools the client was last given, to notice when they change.
    listed: Mutex<Option<Vec<String>>>,
}

/// Pauses between `toolList` tries inside one `tools/list` (about 15 seconds in all), for a
/// session that starts while gxserver restarts.
const TOOL_LIST_RETRY_MS: [u64; 5] = [500, 1_000, 2_000, 4_000, 8_000];
/// After that, how often and how long the watcher keeps asking.
const TOOL_WATCH_EVERY: Duration = Duration::from_secs(5);
const TOOL_WATCH_TRIES: u32 = 120;
/// How often a listed tool set is checked for changes.
const TOOL_CHANGE_CHECK_EVERY: Duration = Duration::from_secs(15);

/// The session this MCP server runs in: `GHOSTEX_GLOBAL_SESSION_REF` (`server:project:session`),
/// else the zmx name in `GHOSTEX_SESSION_ID` (`server-project-session`).
fn session_from_env() -> (Option<String>, Option<String>) {
    let read = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    if let Some(reference) = read("GHOSTEX_GLOBAL_SESSION_REF") {
        let parts: Vec<&str> = reference.split(':').collect();
        if parts.len() == 3 && !parts[1].is_empty() && !parts[2].is_empty() {
            return (Some(parts[1].to_string()), Some(parts[2].to_string()));
        }
    }
    if let Some(name) = read("GHOSTEX_SESSION_ID") {
        let parts: Vec<&str> = name.split('-').collect();
        if parts.len() == 3 && !parts[1].is_empty() && !parts[2].is_empty() {
            return (Some(parts[1].to_string()), Some(parts[2].to_string()));
        }
    }
    (None, None)
}

/// gxserver as the session sees it (`GHOSTEX_GXSERVER_BASE_URL` + its token file), else the
/// CLI's normal local target.
fn target_from_env() -> CliResult<Target> {
    let base_url = std::env::var("GHOSTEX_GXSERVER_BASE_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    let token = std::env::var("GHOSTEX_GXSERVER_AUTH_TOKEN_FILE")
        .ok()
        .and_then(|file| std::fs::read_to_string(file.trim()).ok())
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty());
    match (base_url, token) {
        (Some(base_url), Some(token)) => Ok(Target {
            base_url,
            forward_plan: None,
            kind: "direct".to_string(),
            profile_id: None,
            server_id: None,
            tailcat_plan: None,
            token,
        }),
        _ => resolve_local_gxserver_target(),
    }
}

impl Server {
    fn post(&self, path: &str, params: Value, timeout_ms: u64) -> CliResult<Value> {
        let mut flags = Flags::default();
        flags.insert_text("timeout", &timeout_ms.to_string());
        let body = json!({ "params": params, "protocolVersion": GXSERVER_PROTOCOL_VERSION });
        let envelope = request_gxserver_envelope(&self.target, path, &body, &flags)?;
        Ok(envelope.get("result").cloned().unwrap_or(Value::Null))
    }

    fn session_params(&self) -> Value {
        let mut params = json!({});
        if let Some(project_id) = &self.project_id {
            params["projectId"] = json!(project_id);
        }
        if let Some(session_id) = &self.session_id {
            params["sessionId"] = json!(session_id);
        }
        params
    }

    fn send(&self, message: &Value) {
        let body = serde_json::to_string(message).unwrap_or_else(|_| "null".to_string());
        let framing = *self.framing.lock().unwrap_or_else(PoisonError::into_inner);
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        match framing {
            Framing::ContentLength => {
                let _ = write!(stdout, "Content-Length: {}\r\n\r\n", body.len());
                let _ = stdout.write_all(body.as_bytes());
            }
            Framing::Lines => {
                let _ = stdout.write_all(body.as_bytes());
                let _ = stdout.write_all(b"\n");
            }
        }
        let _ = stdout.flush();
    }

    /// One tool list. A refusal from gxserver (Connectors is off, the session is gone) is an
    /// empty list: the session has no connector tools. Only an unreachable gxserver is an error.
    fn list_tools_once(&self) -> Result<Value, String> {
        match self.post(
            "/api/listConnectorAgentTools",
            self.session_params(),
            TOOL_LIST_TIMEOUT_MS,
        ) {
            Ok(result) => Ok(json!({ "tools": result.get("tools").cloned().unwrap_or(json!([])) })),
            Err(CliError::Rpc { .. }) => Ok(json!({ "tools": [] })),
            Err(error) => Err(error.to_string()),
        }
    }

    fn names(tools: &Value) -> Vec<String> {
        let mut names: Vec<String> = tools
            .get("tools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        names.sort();
        names
    }

    /// Checks the tool set every few seconds and tells the client to list again when it changed.
    fn watch_for_changes(self: &Arc<Self>) {
        let server = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(TOOL_CHANGE_CHECK_EVERY);
            let Ok(tools) = server.list_tools_once() else {
                continue;
            };
            let names = Self::names(&tools);
            let changed = {
                let mut listed = server.listed.lock().unwrap_or_else(PoisonError::into_inner);
                let changed = listed.as_ref().is_some_and(|listed| *listed != names);
                if changed {
                    *listed = Some(names);
                }
                changed
            };
            if changed {
                server.send(&json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/tools/list_changed",
                }));
            }
        });
    }

    /// The tool list, retried while gxserver is unreachable.
    fn tools(&self) -> Result<Value, String> {
        let mut last = self.list_tools_once();
        for pause in TOOL_LIST_RETRY_MS {
            if last.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(pause));
            last = self.list_tools_once();
        }
        if let Ok(tools) = &last {
            *self.listed.lock().unwrap_or_else(PoisonError::into_inner) = Some(Self::names(tools));
        }
        last
    }

    /// Keeps asking gxserver for the tools after a failed `tools/list`, and tells the client to
    /// list them again (`notifications/tools/list_changed`) as soon as it answers.
    fn watch_for_tools(self: &Arc<Self>) {
        if self.watching_tools.swap(true, Ordering::SeqCst) {
            return;
        }
        let server = self.clone();
        std::thread::spawn(move || {
            for _ in 0..TOOL_WATCH_TRIES {
                std::thread::sleep(TOOL_WATCH_EVERY);
                if server.list_tools_once().is_ok() {
                    server.send(&json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/tools/list_changed",
                    }));
                    break;
                }
            }
            server.watching_tools.store(false, Ordering::SeqCst);
        });
    }

    fn call_tool(&self, request_key: &str, params: &Value) -> Value {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let arguments = params
            .get("arguments")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let call_id = uuid::Uuid::new_v4().to_string();
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request_key.to_string(), call_id.clone());
        let mut body = self.session_params();
        body["tool"] = json!(name);
        body["arguments"] = arguments;
        body["callId"] = json!(call_id);
        let result = self.post("/api/callConnectorAgentTool", body, TOOL_CALL_TIMEOUT_MS);
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(request_key);
        match result {
            Ok(result) if result.get("content").is_some() => result,
            Ok(_) => {
                json!({ "content": [{ "type": "text", "text": "Ghostex returned no result." }], "isError": true })
            }
            Err(error) => json!({
                "content": [{ "type": "text", "text": format!("Ghostex could not run {name}: {error}") }],
                "isError": true,
            }),
        }
    }

    fn cancel(&self, request_key: &str) {
        let call_id = self
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(request_key);
        if let Some(call_id) = call_id {
            let _ = self.post(
                "/api/cancelConnectorAgentToolCall",
                json!({ "callId": call_id }),
                TOOL_LIST_TIMEOUT_MS,
            );
        }
    }
}

fn request_key(id: &Value) -> String {
    id.to_string()
}

fn handle(server: &Arc<Server>, message: Value) {
    let Some(object) = message.as_object() else {
        return;
    };
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let id = object.get("id").filter(|id| !id.is_null()).cloned();
    if method == "notifications/cancelled" {
        if let Some(request_id) = params.get("requestId") {
            let server = server.clone();
            let key = request_key(request_id);
            std::thread::spawn(move || server.cancel(&key));
        }
        return;
    }
    let Some(id) = id else {
        // Other notifications need no answer.
        return;
    };
    let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    match method.as_str() {
        "initialize" => {
            let protocol_version = params
                .get("protocolVersion")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| json!("2024-11-05"));
            server.send(&reply(json!({
                "capabilities": { "tools": { "listChanged": true } },
                "protocolVersion": protocol_version,
                "serverInfo": { "name": crate::connectors::CONNECTORS_MCP_SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            })));
        }
        "ping" => server.send(&reply(json!({}))),
        "tools/list" => {
            let server = server.clone();
            std::thread::spawn(move || match server.tools() {
                Ok(tools) => server.send(&json!({ "jsonrpc": "2.0", "id": id, "result": tools })),
                // CDXC:Connectors 2026-10-16 WHY:
                // An unreachable gxserver (a session that started during a restart) is an error, not an empty list: Claude never asks for an empty list again, so the session would stay without its tools for the whole conversation. The watcher announces the tools once gxserver is back.
                Err(error) => {
                    server.send(&json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32603,
                            "message": format!("Ghostex is not reachable right now, so the connector tools are not available yet ({error})."),
                        },
                    }));
                    server.watch_for_tools();
                }
            });
        }
        "tools/call" => {
            let server = server.clone();
            std::thread::spawn(move || {
                let result = server.call_tool(&request_key(&id), &params);
                server.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
            });
        }
        "resources/list" => server.send(&reply(json!({ "resources": [] }))),
        "prompts/list" => server.send(&reply(json!({ "prompts": [] }))),
        _ => server.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("Unknown MCP method: {method}") },
        })),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(header: &str) -> Option<usize> {
    header.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

/// A parsed frame's messages: a JSON-RPC batch (an array) is its members, each handled on its own.
fn push_frame(messages: &mut Vec<Value>, frame: Value) {
    match frame {
        Value::Array(batch) => messages.extend(batch),
        message => messages.push(message),
    }
}

/// Pulls every complete message out of `buffer`, learning the framing from the first one.
fn take_messages(buffer: &mut Vec<u8>, server: &Server) -> Vec<Value> {
    let mut messages = Vec::new();
    loop {
        let start = buffer
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(buffer.len());
        if start > 0 {
            buffer.drain(..start);
        }
        if buffer.is_empty() {
            return messages;
        }
        // A line-framed message is an object or a batch (an array); anything else is a header.
        if buffer[0] == b'{' || buffer[0] == b'[' {
            let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
                return messages;
            };
            let line: Vec<u8> = buffer.drain(..=end).collect();
            *server
                .framing
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Framing::Lines;
            if let Ok(frame) = serde_json::from_slice::<Value>(&line) {
                push_frame(&mut messages, frame);
            }
            continue;
        }
        let Some(header_end) = find(buffer, b"\r\n\r\n") else {
            return messages;
        };
        let header = String::from_utf8_lossy(&buffer[..header_end]).to_string();
        let Some(length) = content_length(&header) else {
            buffer.drain(..header_end + 4);
            continue;
        };
        let body_start = header_end + 4;
        if buffer.len() < body_start + length {
            return messages;
        }
        let body: Vec<u8> = buffer[body_start..body_start + length].to_vec();
        buffer.drain(..body_start + length);
        *server
            .framing
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Framing::ContentLength;
        if let Ok(frame) = serde_json::from_slice::<Value>(&body) {
            push_frame(&mut messages, frame);
        }
    }
}

pub(super) fn run() -> CliResult<()> {
    let (project_id, session_id) = session_from_env();
    let server = Arc::new(Server {
        project_id,
        session_id,
        target: target_from_env()?,
        framing: Mutex::new(Framing::Lines),
        calls: Mutex::new(HashMap::new()),
        watching_tools: AtomicBool::new(false),
        listed: Mutex::new(None),
    });
    server.watch_for_changes();
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    loop {
        let read = match stdin.read(&mut chunk) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        for message in take_messages(&mut buffer, &server) {
            handle(&server, message);
        }
    }
    Ok(())
}
