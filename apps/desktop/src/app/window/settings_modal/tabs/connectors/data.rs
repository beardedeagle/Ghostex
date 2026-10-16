//! Reading the `/api/readConnectors` answer: its workspaces and the plain fields of each part.
use serde_json::Value;

pub(super) fn workspaces(connectors: Option<&Value>) -> Vec<Value> {
    connectors
        .and_then(|connectors| connectors.get("workspaces"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(super) fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool) == Some(true)
}

pub(super) fn list(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(super) fn busy_key(workspace: &str, action: &str) -> String {
    format!("{workspace}:{action}")
}
