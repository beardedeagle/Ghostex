//! An Executor app's tools: listing them, and calling one that only reads.

use serde_json::{json, Value};

use super::apps;
use super::client::{self, Endpoint};

const OUTPUT_LIMIT: usize = 30_000;

fn index(endpoint: &Endpoint, app: &Value) -> Result<Vec<Value>, String> {
    let index = client::request(
        endpoint,
        "GET",
        &format!("/v1/tools/index?app={}", apps::app_id(app)),
        None,
    )?;
    Ok(index
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// One app's tools: name, description and whether Executor marks it read-only.
pub(super) fn list(endpoint: &Endpoint, app: &Value) -> Result<Vec<Value>, String> {
    Ok(index(endpoint, app)?
        .iter()
        .map(|tool| {
            json!({
                "name": tool.get("name").and_then(Value::as_str).unwrap_or_default(),
                "description": tool.get("description").and_then(Value::as_str).unwrap_or_default(),
                "readOnly": tool.get("readOnly").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect())
}

fn clip(text: &str) -> String {
    match text.char_indices().nth(OUTPUT_LIMIT) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_string(),
    }
}

/// Runs a tool Executor marks read-only and returns its result as text.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// The user decided Ghostex asks with its own card before a connector sends, posts, creates, changes or deletes anything; until that card exists, only tools Executor marks read-only run, they are called as queries (Executor refuses a mutation called that way), and a call that comes back asking for approval is never resumed.
pub(super) fn call_read_only(
    endpoint: &Endpoint,
    app: &Value,
    tool: &str,
    input: &Value,
) -> Result<String, String> {
    let tool = tool.trim();
    let found = index(endpoint, app)?
        .into_iter()
        .find(|item| item.get("name").and_then(Value::as_str) == Some(tool))
        .ok_or_else(|| format!("{} has no tool called \"{tool}\".", apps::app_name(app)))?;
    if found.get("readOnly").and_then(Value::as_bool) != Some(true) {
        return Err(format!(
            "{tool} could change something in {} (it isn't marked read-only). Ghostex can't ask you about those yet, so only reads run for now.",
            apps::app_name(app)
        ));
    }
    let result = client::request(
        endpoint,
        "POST",
        "/v1/tools/call",
        Some(&json!({
            "app": apps::app_id(app),
            "tool": tool,
            "kind": "query",
            "input": input,
        })),
    )?;
    match result.get("status").and_then(Value::as_str) {
        Some("completed") => {
            let value = result.get("value").cloned().unwrap_or(Value::Null);
            let text = match &value {
                Value::String(text) => text.clone(),
                other => serde_json::to_string_pretty(other).unwrap_or_default(),
            };
            let failed = result.get("toolError").and_then(Value::as_bool) == Some(true);
            Ok(format!("{}{}", if failed { "Error: " } else { "" }, clip(&text)))
        }
        Some("approval-required") => Err(format!(
            "{} asks for approval before {tool} runs. Ghostex can't ask you about those yet, so it didn't run.",
            apps::app_name(app)
        )),
        _ => Err(format!(
            "Executor answered {tool} in a way Ghostex doesn't know: {}",
            clip(&result.to_string())
        )),
    }
}
