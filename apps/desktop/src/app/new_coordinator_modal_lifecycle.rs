//! Open, close, and create for the native New Coordinator dialog.
//! SEE-ALSO: apps/desktop/src/app/window/new_coordinator_modal.rs (the window), apps/desktop/src/app/gx_store/create/coordinator.rs (the create), packages/gx-core/src/sidebar_actions/open.rs (the `coordinator` project action that opens it).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the dialog for the `newCoordinator` modal: `groupId` (the project's sidebar group,
    /// which names the machine too) and `projectName`. An open without a group is dropped.
    pub(crate) fn open_gpui_new_coordinator_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(group_id) = text("groupId") else {
            return;
        };
        let agents = self.gx_store_coordinator_agents();
        // The machine whose gxserver creates the orchestrator also keeps its Orchestrator
        // instructions; an unconnected remote one shows none.
        let project = ghostex_gx_core::ProjectKey::parse_sidebar_group_id(&group_id);
        let instructions_target =
            project
                .as_ref()
                .and_then(|project| match project.machine.remote_id() {
                    None => Some(None),
                    Some(machine_id) => self
                        .gpui_remote_gxserver_request_target(machine_id)
                        .map(Some),
                });
        let project_id = project.map(|project| project.project_id);
        let config = NewCoordinatorModalConfig {
            project_name: text("projectName").unwrap_or_else(|| "this project".to_string()),
            agents,
            selected_agent: 0,
            instructions: None,
            palette: self.gpui_native_modal_palette(),
        };
        let save_target = instructions_target.clone();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            match command {
                NewCoordinatorModalCommand::Create {
                    agent_id,
                    name,
                    model,
                    effort,
                } => app.gx_store_create_coordinator(
                    &group_id,
                    &agent_id,
                    &name,
                    model.as_deref(),
                    effort.as_deref(),
                    cx,
                ),
                // The dialog stays open until gxserver answers the save.
                NewCoordinatorModalCommand::SaveInstructions {
                    project_id,
                    instructions,
                    reply,
                } => {
                    let Some(remote) = save_target.clone() else {
                        reply.finish(Err("That computer is not connected.".to_string()), cx);
                        return;
                    };
                    let mut params = serde_json::json!({ "instructions": instructions });
                    if let Some(project_id) = project_id {
                        params["projectId"] = serde_json::json!(project_id);
                    }
                    cx.spawn(async move |_this, cx| {
                        let saved = crate::app::gx_store::gx_rpc(
                            remote,
                            "/api/setCoordinatorDefaultInstructions",
                            params,
                        )
                        .await
                        .map_err(|error| error.message);
                        cx.update(|cx| reply.finish(saved, cx));
                    })
                    .detach();
                    return;
                }
                NewCoordinatorModalCommand::Cancel => {}
            }
            app.release_native_app_modal_window(GpuiAppModalKind::NewCoordinator, cx);
        });
        let dialog: std::rc::Rc<
            std::cell::RefCell<Option<gpui::WeakEntity<GpuiNewCoordinatorModalWindow>>>,
        > = Default::default();
        let dialog_slot = dialog.clone();
        self.open_native_app_modal(
            GpuiAppModalKind::NewCoordinator,
            NEW_COORDINATOR_MODAL_WIDTH,
            NEW_COORDINATOR_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                let view =
                    cx.new(|cx| GpuiNewCoordinatorModalWindow::new(config, host, window, cx));
                *dialog_slot.borrow_mut() = Some(view.downgrade());
                view
            },
            cx,
        );
        if let (Some(remote), Some(project_id)) = (instructions_target, project_id) {
            cx.spawn(async move |_this, cx| {
                let read = crate::app::gx_store::gx_rpc(
                    remote,
                    "/api/readCoordinatorDefaultInstructions",
                    serde_json::json!({ "projectId": project_id }),
                )
                .await;
                // An older gxserver without Orchestrator instructions leaves the row out.
                let Ok(view) = read else {
                    return;
                };
                let info = OrchestratorInstructionsInfo::from_view(&view);
                let Some(dialog) = dialog.borrow().as_ref().and_then(|dialog| dialog.upgrade())
                else {
                    return;
                };
                cx.update(|cx| {
                    dialog.update(cx, |dialog, cx| dialog.set_instructions(Some(info), cx))
                });
            })
            .detach();
        }
    }

    /// Opens the Make Coordinator dialog for the `makeCoordinator` modal: `sessionId` (the sidebar
    /// row's id) and `sessionTitle`. An open without a session is dropped.
    pub(crate) fn open_gpui_make_coordinator_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(sidebar_session_id) = text("sessionId") else {
            return;
        };
        let session_title = text("sessionTitle").unwrap_or_else(|| "This session".to_string());
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            if let MakeCoordinatorModalCommand::Make { goal } = command {
                app.gx_store_promote_coordinator(&sidebar_session_id, &goal, cx);
            }
            app.release_native_app_modal_window(GpuiAppModalKind::MakeCoordinator, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::MakeCoordinator,
            NEW_COORDINATOR_MODAL_WIDTH,
            MAKE_COORDINATOR_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiMakeCoordinatorModalWindow::new(session_title, palette, host, window, cx)
                })
            },
            cx,
        );
    }
}
