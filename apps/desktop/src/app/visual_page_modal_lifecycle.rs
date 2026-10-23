//! Open and close plumbing for the floating window a chat page card opens.
//! SEE-ALSO: apps/desktop/src/app/window/visual_page_modal.rs (the window entity), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/cef_deferred_startup.rs (an open that waits for the web runtime).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::web_bridge_types::AppModalHostBridgeEvent;
use crate::*;

impl GhostexGpuiApp {
    /// A chat page card's Open for the session `session_id`. A page a remote machine's session
    /// published lives on that machine's gxserver, so its address is resolved for the machine
    /// (`gpui_remote_machine_url`) before the window opens; the open message then carries the
    /// machine (`remoteMachineId`, `remoteProjectId`) for the window's picture and its links.
    pub(crate) fn open_session_chat_visual_page(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let open = |app: &mut Self, message: &serde_json::Value, window: &mut Window, cx| {
            app.receive_app_modal_host_bridge_event(
                AppModalHostBridgeEvent::Message(message.to_string()),
                window,
                cx,
            );
        };
        let Some(remote_project) = self
            .remote_project_for_terminal_target(GpuiEngineTerminalEventTarget::Agents(session_id))
        else {
            open(self, message, window, cx);
            return;
        };
        let url = message["url"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        let Some(target) =
            self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
        else {
            let machine = gpui_remote_machine_name_from_settings(&remote_project.remote_machine_id)
                .unwrap_or_else(|| "the remote computer".to_string());
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "This page is on another computer",
                &format!("Connect {machine} to open it."),
                cx,
            );
            return;
        };
        match gpui_remote_machine_url(&url, &target) {
            GpuiRemoteMachineUrl::Gxserver(url) => {
                let mut message = message.clone();
                message["url"] = url.into();
                message["remoteMachineId"] = remote_project.remote_machine_id.into();
                message["remoteProjectId"] = remote_project.project_id.into();
                open(self, &message, window, cx);
            }
            GpuiRemoteMachineUrl::MachineLoopback => {
                self.open_remote_machine_loopback_link(&url, remote_project, cx);
            }
            GpuiRemoteMachineUrl::AsWritten => open(self, message, window, cx),
        }
    }

    /// Opens the floating window for the `visualPage` open message the chat's page card sends
    /// (`url`, `title`, and the published file's name as `file`). The page needs the web runtime:
    /// until it runs, the open is held (and the install prompt shown when it is not installed).
    pub(crate) fn open_gpui_visual_page_modal(
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
        let Some(url) = text("url").filter(|url| {
            gpui::http_client::Url::parse(url)
                .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
        }) else {
            return;
        };
        if !cef::context_initialized() {
            self.defer_gpui_app_modal_open_for_cef(
                GpuiAppModalKind::VisualPage,
                message.clone(),
                cx,
            );
            return;
        }
        let title = text("title").unwrap_or_else(|| "Page".to_string());
        let detail = match text("file") {
            Some(file) => format!("Interactive HTML page · {file}"),
            None => "Interactive HTML page".to_string(),
        };
        let remote_project =
            text("remoteMachineId").map(|remote_machine_id| GpuiRemoteProjectReference {
                remote_machine_id,
                project_id: text("remoteProjectId").unwrap_or_default(),
            });
        let remote = remote_project.as_ref().and_then(|remote_project| {
            self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
        });
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_visual_page_modal_command(command, remote_project.clone(), cx);
        });
        let kind = GpuiAppModalKind::VisualPage;
        let (width, height) = self.chat_preview_modal_size(cx);
        self.open_native_app_modal(
            kind,
            width,
            height,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiVisualPageModalWindow::new(
                        url, title, detail, palette, host, remote, window, cx,
                    )
                })
            },
            cx,
        );
    }

    /// `remote_project` is the machine whose session published the open page, `None` for this
    /// computer's.
    fn handle_gpui_visual_page_modal_command(
        &mut self,
        command: VisualPageModalCommand,
        remote_project: Option<GpuiRemoteProjectReference>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.native_app_modal_kind() != Some(GpuiAppModalKind::VisualPage) {
            return;
        }
        let url = match command {
            VisualPageModalCommand::Close => {
                self.close_native_app_modal_from_bridge(cx);
                return;
            }
            VisualPageModalCommand::OpenInBrowser { url } => {
                self.close_native_app_modal_from_bridge(cx);
                url
            }
            VisualPageModalCommand::RefreshFailed { message } => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "The page could not be refreshed",
                    &message,
                    cx,
                );
                return;
            }
            // A link the page opened goes to the reader's browser and leaves the page open,
            // unless it names the page's machine's own localhost, which only the Browser on that
            // machine's network reaches.
            VisualPageModalCommand::OpenLink { url } => {
                let target = remote_project.as_ref().and_then(|remote_project| {
                    self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
                });
                match (remote_project, target) {
                    (Some(remote_project), Some(target)) => {
                        match gpui_remote_machine_url(&url, &target) {
                            GpuiRemoteMachineUrl::Gxserver(url) => url,
                            GpuiRemoteMachineUrl::MachineLoopback => {
                                self.close_native_app_modal_from_bridge(cx);
                                self.open_remote_machine_loopback_link(&url, remote_project, cx);
                                return;
                            }
                            GpuiRemoteMachineUrl::AsWritten => url,
                        }
                    }
                    (Some(_), None) if gpui_remote_link_is_loopback(&url) => return,
                    _ => url,
                }
            }
        };
        let _ = gpui_open_external_http_url(&url);
    }
}
