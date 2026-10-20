use std::collections::HashMap;
use std::io::{Read, Write};

use serde_json::{json, Value};

use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

// ---------------------------------------------------------------------------
// MCP stdio server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub(super) struct McpServerOptions {
    pub(super) port: Option<u64>,
    pub(super) target: Option<String>,
    pub(super) timeout_ms: u64,
}

pub(super) struct McpState {
    pub(super) captures: HashMap<String, Vec<Value>>,
    pub(super) clients: HashMap<String, CdpClient>,
    pub(super) options: McpServerOptions,
    pub(super) ref_maps: HashMap<String, HashMap<String, String>>,
    pub(super) selected_page_id: Option<String>,
}

impl McpState {
    pub(super) fn new(options: McpServerOptions) -> Self {
        let selected_page_id = options.target.clone();
        McpState {
            captures: HashMap::new(),
            clients: HashMap::new(),
            options,
            ref_maps: HashMap::new(),
            selected_page_id,
        }
    }
}

pub(super) fn run_browser_devtools_mcp_server(options: McpServerOptions) -> CliResult<()> {
    let mut state = McpState::new(options);
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
        for message in extract_mcp_messages(&mut buffer) {
            if let Some(response) = handle_browser_mcp_message(&message, &mut state) {
                send_mcp_message(&response);
            }
        }
    }
    Ok(())
}

fn send_mcp_message(message: &Value) {
    let body = serde_json::to_string(message).unwrap_or_else(|_| "null".to_string());
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let _ = write!(stdout, "Content-Length: {}\r\n\r\n", body.len());
    let _ = stdout.write_all(body.as_bytes());
    let _ = stdout.flush();
}

/// Content-Length framed message extraction (exact McpStdioTransport.read
/// port): a header block without a Content-Length line drops the buffer.
pub(super) fn extract_mcp_messages(buffer: &mut Vec<u8>) -> Vec<Value> {
    let mut messages = Vec::new();
    loop {
        let Some(header_end) = find_subsequence(buffer, b"\r\n\r\n") else {
            return messages;
        };
        let header = String::from_utf8_lossy(&buffer[..header_end]).to_string();
        let Some(length) = content_length_from_header(&header) else {
            buffer.clear();
            return messages;
        };
        let body_start = header_end + 4;
        if buffer.len() < body_start + length {
            return messages;
        }
        let body = String::from_utf8_lossy(&buffer[body_start..body_start + length]).to_string();
        buffer.drain(..body_start + length);
        if let Ok(message) = serde_json::from_str::<Value>(&body) {
            messages.push(message);
        }
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// `/^Content-Length:\s*(\d+)/im` over the header block.
fn content_length_from_header(header: &str) -> Option<usize> {
    for line in header.split("\r\n") {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            let rest_original = &line[line.len() - rest.len()..];
            let trimmed = rest_original.trim_start();
            let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                continue;
            }
            if let Ok(length) = digits.parse::<usize>() {
                return Some(length);
            }
        }
    }
    None
}

pub(super) fn handle_browser_mcp_message(message: &Value, state: &mut McpState) -> Option<Value> {
    if !message.is_object() {
        return None;
    }
    let id = message.get("id").cloned();
    let method = js_string_of(message.get("method"));
    let method = method.as_str();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let id = match id {
        None | Some(Value::Null) => {
            return None;
        }
        Some(id) => id,
    };
    if method == "notifications/initialized" {
        return None;
    }
    let outcome: CliResult<Value> = match method {
        "initialize" => {
            let protocol_version = params
                .get("protocolVersion")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| Value::String("2024-11-05".to_string()));
            Ok(json!({
                "capabilities": { "tools": {} },
                "protocolVersion": protocol_version,
                "serverInfo": { "name": "ghostex-browser-devtools", "version": "1.0.0" },
            }))
        }
        "tools/list" => Ok(json!({ "tools": browser_mcp_tools() })),
        "tools/call" => {
            let name = params.get("name").cloned();
            let arguments = params
                .get("arguments")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| json!({}));
            call_browser_mcp_tool(name.as_ref(), &arguments, state)
        }
        _ => {
            return Some(json!({
                "error": { "code": -32601, "message": format!("Unknown MCP method: {method}") },
                "id": id,
                "jsonrpc": "2.0",
            }));
        }
    };
    match outcome {
        Ok(result) => Some(json!({ "id": id, "jsonrpc": "2.0", "result": result })),
        Err(error) => Some(json!({
            "error": { "code": -32000, "message": error.to_string() },
            "id": id,
            "jsonrpc": "2.0",
        })),
    }
}

pub(super) fn browser_mcp_tools() -> Value {
    let page_selector_properties = json!({
        "pageId": { "description": "CDP target id. Defaults to the selected page, then the first Ghostex CEF page.", "type": "string" },
        "titleContains": { "description": "Select a page whose title contains this text.", "type": "string" },
        "urlContains": { "description": "Select a page whose URL contains this text.", "type": "string" },
    });
    let props = |extra: Value| -> Value {
        let mut merged = page_selector_properties
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                merged.insert(key.clone(), value.clone());
            }
        }
        Value::Object(merged)
    };
    json!([
        {
            "name": "ghostex_list_pages",
            "description": "List embedded Ghostex CEF pages available over the local DevTools endpoint.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "ghostex_select_page",
            "description": "Select the embedded page used by subsequent Ghostex browser tools.",
            "inputSchema": { "type": "object", "properties": props(json!({ "index": { "type": "number" } })) },
        },
        {
            "name": "ghostex_navigate",
            "description": "Navigate a Ghostex embedded browser page.",
            "inputSchema": { "type": "object", "required": ["url"], "properties": props(json!({ "url": { "type": "string" } })) },
        },
        {
            "name": "ghostex_evaluate",
            "description": "Evaluate JavaScript in the selected embedded browser page.",
            "inputSchema": {
                "type": "object",
                "required": ["script"],
                "properties": props(json!({ "awaitPromise": { "type": "boolean" }, "script": { "type": "string" } })),
            },
        },
        {
            "name": "ghostex_console_logs",
            "description": "Read captured console, exception, and browser log entries for the selected embedded page.",
            "inputSchema": {
                "type": "object",
                "properties": props(json!({ "clear": { "type": "boolean" }, "limit": { "type": "number" } })),
            },
        },
        {
            "name": "ghostex_snapshot",
            "description": "Return an agent-friendly snapshot of visible interactive elements and assign @e refs.",
            "inputSchema": { "type": "object", "properties": props(json!({ "limit": { "type": "number" } })) },
        },
        {
            "name": "ghostex_click",
            "description": "Click an element by @e ref from ghostex_snapshot or a CSS selector.",
            "inputSchema": {
                "type": "object",
                "properties": props(json!({ "ref": { "type": "string" }, "selector": { "type": "string" } })),
            },
        },
        {
            "name": "ghostex_fill",
            "description": "Fill an input, textarea, select, or contenteditable element by @e ref or CSS selector.",
            "inputSchema": {
                "type": "object",
                "required": ["text"],
                "properties": props(json!({ "ref": { "type": "string" }, "selector": { "type": "string" }, "text": { "type": "string" } })),
            },
        },
        {
            "name": "ghostex_press_key",
            "description": "Send a keyboard key to the selected embedded browser page.",
            "inputSchema": { "type": "object", "required": ["key"], "properties": props(json!({ "key": { "type": "string" } })) },
        },
        {
            "name": "ghostex_screenshot",
            "description": "Capture the selected embedded browser viewport as a PNG image.",
            "inputSchema": { "type": "object", "properties": props(json!({})) },
        },
    ])
}

fn call_browser_mcp_tool(
    name: Option<&Value>,
    args: &Value,
    state: &mut McpState,
) -> CliResult<Value> {
    let name = js_string_of(name);
    match name.as_str() {
        "ghostex_list_pages" => Ok(text_tool_result(&browser_mcp_list_pages(state)?)),
        "ghostex_select_page" => Ok(text_tool_result(&browser_mcp_select_page(args, state)?)),
        "ghostex_navigate" => Ok(text_tool_result(&browser_mcp_navigate(args, state)?)),
        "ghostex_evaluate" => Ok(text_tool_result(&browser_mcp_evaluate(args, state)?)),
        "ghostex_console_logs" => Ok(text_tool_result(&browser_mcp_console_logs(args, state)?)),
        "ghostex_snapshot" => Ok(text_tool_result(&browser_mcp_snapshot(args, state)?)),
        "ghostex_click" => Ok(text_tool_result(&browser_mcp_click(args, state)?)),
        "ghostex_fill" => Ok(text_tool_result(&browser_mcp_fill(args, state)?)),
        "ghostex_press_key" => Ok(text_tool_result(&browser_mcp_press_key(args, state)?)),
        "ghostex_screenshot" => {
            let value = browser_mcp_screenshot(args, state)?;
            Ok(image_tool_result(&value))
        }
        other => Err(CliError::Other(format!(
            "Unknown Ghostex browser MCP tool: {other}"
        ))),
    }
}

fn text_tool_result(value: &Value) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn image_tool_result(value: &Value) -> Value {
    let summary = json!({
        "page": value.get("page").cloned().unwrap_or(Value::Null),
        "size": value.get("size").cloned().unwrap_or(Value::Null),
    });
    let text = serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "null".to_string());
    json!({
        "content": [
            { "type": "text", "text": text },
            {
                "type": "image",
                "data": value.get("data").cloned().unwrap_or(Value::Null),
                "mimeType": "image/png",
            },
        ],
    })
}
