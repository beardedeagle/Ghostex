//! Open and close plumbing for the native Send Feedback pop-up, shared by the desktop and the GPUI
//! web build (symlinked there).
//! SEE-ALSO: apps/desktop/src/app/window/feedback_modal/ (the pop-up and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs and apps/gpui-web/src/app/web_host/modals.rs (the two window openers).
use crate::app::gx_store::read_preference_value;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the pop-up for the `feedback` modal kind, with the GitHub username the last feedback
    /// named. It needs nothing from the open message: the pop-up drafts and sends through gxserver
    /// itself and only tells the app when it closed.
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
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| match command {
            FeedbackModalCommand::Closed => {
                app.release_native_app_modal_window(GpuiAppModalKind::Feedback, cx)
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
