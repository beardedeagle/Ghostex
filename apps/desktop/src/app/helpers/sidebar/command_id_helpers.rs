// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds command-title dedupe/key helpers, sidebar button-id/order helpers, default-agent lookups, and stored agent/command JSON value builders.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;

pub(crate) fn gpui_sidebar_command_short_title(value: String) -> String {
    value.chars().take(20).collect()
}

pub(crate) fn gpui_normalized_sidebar_command_title(value: Option<&str>) -> Option<String> {
    let normalized = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn gpui_sidebar_order_sync_result_message(
    kind: &'static str,
    request_id: &str,
    status: &'static str,
    item_ids: Vec<String>,
) -> serde_json::Value {
    serde_json::json!({
        "itemIds": item_ids,
        "kind": kind,
        "requestId": request_id,
        "status": status,
        "type": "sidebarOrderSyncResult",
    })
}

pub(crate) fn gpui_default_sidebar_agent_by_id(
    agent_id: &str,
) -> Option<&'static GpuiDefaultSidebarAgent> {
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
}

pub(crate) fn gpui_default_sidebar_agent_by_icon(
    icon: &str,
) -> Option<&'static GpuiDefaultSidebarAgent> {
    if icon == "browser" {
        return None;
    }
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.icon == icon)
}

pub(crate) fn gpui_stored_sidebar_commands_value(
    commands: &[GpuiStoredSidebarCommand],
) -> serde_json::Value {
    serde_json::Value::Array(
        commands
            .iter()
            .map(|command| {
                let mut item = serde_json::Map::new();
                item.insert(
                    "actionType".to_string(),
                    serde_json::Value::String(command.action_type.to_string()),
                );
                item.insert(
                    "closeTerminalOnExit".to_string(),
                    serde_json::Value::Bool(command.close_terminal_on_exit),
                );
                if let Some(command_text) = command.command.as_ref() {
                    item.insert(
                        "command".to_string(),
                        serde_json::Value::String(command_text.clone()),
                    );
                }
                item.insert(
                    "commandId".to_string(),
                    serde_json::Value::String(command.command_id.clone()),
                );
                if let Some(icon) = command.icon.as_ref() {
                    item.insert("icon".to_string(), serde_json::Value::String(icon.clone()));
                }
                item.insert(
                    "isDefault".to_string(),
                    serde_json::Value::Bool(command.is_default),
                );
                item.insert(
                    "name".to_string(),
                    serde_json::Value::String(command.name.clone()),
                );
                item.insert(
                    "playCompletionSound".to_string(),
                    serde_json::Value::Bool(command.play_completion_sound),
                );
                item.insert(
                    "showOnProjectRow".to_string(),
                    serde_json::Value::Bool(command.show_on_project_row),
                );
                if let Some(url) = command.url.as_ref() {
                    item.insert("url".to_string(), serde_json::Value::String(url.clone()));
                }
                serde_json::Value::Object(item)
            })
            .collect(),
    )
}

pub(crate) fn gpui_string_array_value(items: &[String]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|item| serde_json::Value::String(item.clone()))
            .collect(),
    )
}
