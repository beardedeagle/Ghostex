//! Open and save for the native Goal & Instructions and Memory editors of an orchestrator.
//! SEE-ALSO: apps/desktop/src/app/window/coordinator_details_modal.rs (the window), apps/desktop/src/app/gx_store/coordinator_actions.rs (the menu rows that open it).
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};

impl GhostexGpuiApp {
    /// Opens the editor for an orchestrator read from `/api/readCoordinator` (`coordinator`).
    pub(crate) fn open_gpui_coordinator_details_modal(
        &mut self,
        mode: CoordinatorDetailsMode,
        remote: Option<GpuiRemoteGxserverRequestTarget>,
        coordinator: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            coordinator
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let project_id = text("projectId");
        let session_id = text("sessionId");
        let title = text("title");
        let config = CoordinatorDetailsModalConfig {
            mode,
            title: if title.trim().is_empty() {
                "this orchestrator".to_string()
            } else {
                title
            },
            goal: text("goal"),
            instructions: text("instructions"),
            notes: coordinator
                .get("memory")
                .and_then(Value::as_array)
                .map(|notes| {
                    notes
                        .iter()
                        .filter_map(|note| note.get("text").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            palette: self.gpui_native_modal_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            let update = match command {
                CoordinatorDetailsModalCommand::SaveGoal { goal, instructions } => Some(json!({
                    "projectId": project_id, "sessionId": session_id,
                    "goal": goal, "instructions": instructions,
                })),
                CoordinatorDetailsModalCommand::SaveMemory { notes } => Some(json!({
                    "projectId": project_id, "sessionId": session_id, "memory": notes,
                })),
                CoordinatorDetailsModalCommand::Cancel => None,
            };
            if let Some(params) = update {
                let remote = remote.clone();
                cx.spawn(async move |this, cx| {
                    let saved = gx_rpc(remote, "/api/updateCoordinator", params).await;
                    if let Err(error) = saved {
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_gpui_app_modal_toast(
                                "warning",
                                "Orchestrator not updated",
                                &error.message,
                                cx,
                            );
                        });
                    }
                })
                .detach();
            }
            app.release_native_app_modal_window(GpuiAppModalKind::CoordinatorDetails, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::CoordinatorDetails,
            COORDINATOR_DETAILS_MODAL_WIDTH,
            COORDINATOR_DETAILS_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiCoordinatorDetailsModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }
}
