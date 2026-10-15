//! Open and close plumbing for the native Send Bug / Feedback pop-up, shared by the desktop and the GPUI
//! web build (symlinked there).
//! SEE-ALSO: apps/desktop/src/app/window/feedback_modal/ (the pop-up and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs and apps/gpui-web/src/app/web_host/modals.rs (the two window openers).
use crate::app::gx_store::{read_preference_value, read_primary_agent_launcher_id};
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the pop-up for the `feedback` modal kind, with the GitHub username the last feedback
    /// named and the default agent for new sessions. It needs nothing from the open message: the
    /// pop-up drafts, sends and starts the feedback agent through gxserver itself, and tells the app
    /// when it closed or which agent session to open.
    pub(crate) fn open_gpui_feedback_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let config = FeedbackModalConfig {
            app: if cfg!(target_family = "wasm") {
                "web"
            } else {
                "desktop"
            },
            palette: self.gpui_native_modal_palette(),
            github_username: read_preference_value(FEEDBACK_GITHUB_USERNAME_STORAGE_KEY)
                .ok()
                .flatten()
                .unwrap_or_default(),
            agent_id: read_primary_agent_launcher_id(),
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| match command {
            FeedbackModalCommand::Closed => {
                app.release_native_app_modal_window(GpuiAppModalKind::Feedback, cx)
            }
            FeedbackModalCommand::OpenSession {
                project_id,
                session_id,
                agent_id,
            } => {
                app.release_native_app_modal_window(GpuiAppModalKind::Feedback, cx);
                let interface = app.gx_store_preferred_interface(&agent_id);
                app.gx_store_focus_created_session(
                    &project_id,
                    &session_id,
                    true,
                    Some(&interface),
                    cx,
                );
            }
        });
        self.open_native_app_modal(
            GpuiAppModalKind::Feedback,
            FEEDBACK_MODAL_WIDTH,
            FEEDBACK_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiFeedbackModalWindow::new(config, host, window, cx)),
            cx,
        );
    }
}
