//! Opening the File diff dialog (window/file_diff_modal/), which Files uses to compare an open
//! document with the copy on disk.
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the File diff dialog for a `gitFileDiff` message; a newer one for an open File diff
    /// dialog replaces what it shows.
    pub(crate) fn open_gpui_git_file_diff_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(draft) = message
            .get("gitFileDiff")
            .and_then(GitFileDiffDraft::from_json)
        else {
            return;
        };
        let kind = GpuiAppModalKind::GitFileDiff;
        if self.native_app_modal_kind() == Some(kind) {
            let draft = draft.clone();
            if self
                .update_native_app_modal(
                    kind,
                    cx,
                    |modal: &mut GpuiGitFileDiffModalWindow, _window, cx| {
                        modal.set_draft(draft, cx)
                    },
                )
                .is_some()
            {
                return;
            }
        }
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| match command {
            GitFileDiffModalCommand::Close => app.release_native_app_modal_window(kind, cx),
        });
        self.open_native_app_modal(
            kind,
            GIT_FILE_DIFF_MODAL_WIDTH,
            GIT_FILE_DIFF_MODAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiGitFileDiffModalWindow::new(draft, palette, host, window, cx))
            },
            cx,
        );
    }
}
