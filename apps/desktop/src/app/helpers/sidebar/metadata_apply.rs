// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the agent/command metadata-write apply and next-state computation functions.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;

use crate::app::helpers::*;

pub(crate) fn gpui_apply_sidebar_agent_metadata_write(
    write: GpuiSidebarAgentMetadataWrite,
    active_project_id: Option<String>,
) -> Result<Option<serde_json::Value>, String> {
    let params = gpui_sidebar_agent_mutation_params(&write, active_project_id);
    let result = gpui_gxserver_rpc_result(
        "/api/mutateSidebarHudSettings",
        &params,
        Duration::from_secs(10),
    )?;
    let item_ids = gpui_sidebar_metadata_mutation_item_ids(&result);
    Ok(match write {
        GpuiSidebarAgentMetadataWrite::SyncOrder { request_id, .. } => Some(
            gpui_sidebar_order_sync_result_message("agent", &request_id, "success", item_ids),
        ),
        _ => None,
    })
}

pub(crate) fn gpui_apply_sidebar_command_metadata_write(
    write: GpuiSidebarCommandMetadataWrite,
    remote_target: Option<crate::GpuiRemoteGxserverRequestTarget>,
) -> Result<Option<serde_json::Value>, String> {
    let mut params = gpui_sidebar_command_mutation_params(&write);
    let remote_project = (write.scope() == GpuiSidebarCommandScope::Project)
        .then(|| {
            gpui_remote_project_reference_from_project_id(
                gpui_sidebar_command_write_active_project_id(&write),
            )
        })
        .flatten();
    let result = if let Some(project) = remote_project {
        let target = remote_target.ok_or_else(|| "Remote machine is not connected".to_string())?;
        params["activeProjectId"] = serde_json::Value::String(project.project_id);
        gpui_remote_gxserver_rpc_result(
            &target,
            "/api/mutateSidebarHudSettings",
            &params,
            Duration::from_secs(10),
        )?
    } else {
        gpui_gxserver_rpc_result(
            "/api/mutateSidebarHudSettings",
            &params,
            Duration::from_secs(10),
        )?
    };
    let item_ids = gpui_sidebar_metadata_mutation_item_ids(&result);
    Ok(match write {
        GpuiSidebarCommandMetadataWrite::SyncOrder { request_id, .. } => Some(
            gpui_sidebar_order_sync_result_message("command", &request_id, "success", item_ids),
        ),
        _ => None,
    })
}

pub(crate) fn gpui_sidebar_command_write_active_project_id(
    write: &GpuiSidebarCommandMetadataWrite,
) -> &str {
    match write {
        GpuiSidebarCommandMetadataWrite::Save {
            active_project_id, ..
        }
        | GpuiSidebarCommandMetadataWrite::Delete {
            active_project_id, ..
        }
        | GpuiSidebarCommandMetadataWrite::SyncOrder {
            active_project_id, ..
        } => active_project_id,
    }
}
