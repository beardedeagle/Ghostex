// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds the Kanban project board bridge: workarea URL derivation,
// bridge runtime context, board command intents/requests, and the project
// beads bridge response/error helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct ProjectBoardBridgeRuntimeContext {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: String,
    pub(crate) remote_machine_id: Option<String>,
    pub(crate) remote_target: Option<GpuiRemoteGxserverRequestTarget>,
}

pub(crate) fn project_board_bridge_runtime_context_from_snapshot(
    snapshot: Option<&GpuiProjectSnapshot>,
) -> Option<ProjectBoardBridgeRuntimeContext> {
    /*
    CDXC:CefRuntime 2026-06-24-11:03:
    Kanban and Automate CEF bridge execution may carry only explicit active-project identity from the live sidebar snapshot. The bridge sends gxserver the current project id/path pair for scoped Beads and automation operations, never derives projects from .git/folders/URLs, never launches bd directly, and never logs or persists the private path.
    */
    let snapshot = snapshot?;
    if snapshot.is_quick_projectless
        || (!snapshot.feature_availability.kanban && !snapshot.feature_availability.automate)
    {
        return None;
    }
    let project_path = snapshot
        .in_memory_project_path
        .as_ref()?
        .to_string_lossy()
        .trim()
        .to_string();
    if project_path.is_empty() {
        return None;
    }
    let active_project_id = snapshot
        .active_project_id
        .as_ref()
        .map(|project_id| project_id.0.clone());
    let remote_reference = active_project_id
        .as_deref()
        .and_then(gpui_remote_project_reference_from_project_id);
    Some(ProjectBoardBridgeRuntimeContext {
        project_id: remote_reference
            .as_ref()
            .map(|reference| reference.project_id.clone())
            .or(active_project_id),
        project_path,
        remote_machine_id: remote_reference.map(|reference| reference.remote_machine_id),
        remote_target: None,
    })
}

pub(crate) fn project_beads_gxserver_action_for_board_action(
    action: &str,
) -> Result<&'static str, String> {
    match action {
        "addComment" => Ok("comment"),
        "addLabel" => Ok("addLabel"),
        "configGet" => Ok("configGet"),
        "configGetIssuePrefix" => Ok("configGetIssuePrefix"),
        "configSet" => Ok("configSet"),
        "create" => Ok("create"),
        "delete" => Ok("delete"),
        "depAdd" => Ok("depAdd"),
        "depRemove" => Ok("depRemove"),
        "list" => Ok("list"),
        "listIssues" => Ok("board"),
        "listAllLabels" => Ok("listAllLabels"),
        "renamePrefix" => Ok("renamePrefix"),
        "removeLabel" => Ok("removeLabel"),
        "search" => Ok("search"),
        "setLabels" => Ok("setLabels"),
        "show" => Ok("show"),
        "updateDescription" => Ok("updateDescription"),
        "updateEstimate" => Ok("updateEstimate"),
        "updatePriority" => Ok("updatePriority"),
        "updateStatus" => Ok("updateStatus"),
        "updateTitle" => Ok("updateTitle"),
        _ => Err(format!("Unsupported Beads action: {action}")),
    }
}

pub(crate) fn project_beads_bridge_response_from_gxserver(
    status_code: u16,
    body: &str,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    if !(200..300).contains(&status_code) {
        let error = project_beads_gxserver_error_message(body, Some(status_code));
        return Ok(project_beads_bridge_error_response(request_id, &error));
    }
    let response = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| "gxserver returned an invalid Beads response.".to_string())?;
    let result = response
        .get("result")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not return a Beads result.".to_string())?;
    Ok(project_beads_bridge_response_from_result(
        &serde_json::Value::Object(result.clone()),
        request_id,
    ))
}

pub(crate) fn project_beads_gxserver_error_message(body: &str, status_code: Option<u16>) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
    {
        return message.to_string();
    }
    match status_code {
        Some(status_code) => format!("gxserver Beads request failed with HTTP {status_code}."),
        None => "gxserver Beads request failed.".to_string(),
    }
}
