//! Settings > Integrations' SpaceO buttons: install or update, reinstall, uninstall and the
//! explicit update check, run like Trycua's (cua_gte_and_file_open.rs).

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_spaceo_install_or_update(
        &mut self,
        _window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.refuse_gpui_spaceo_on_unsupported_host(cx) {
            return;
        }
        self.start_gpui_spaceo_job(gpui_spaceo_command_action(), cx);
    }

    pub(crate) fn handle_gpui_spaceo_reinstall(
        &mut self,
        _window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.refuse_gpui_spaceo_on_unsupported_host(cx) {
            return;
        }
        self.start_gpui_spaceo_job(gpui_spaceo_reinstall_command_action(), cx);
    }

    pub(crate) fn handle_gpui_spaceo_uninstall(
        &mut self,
        _window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.refuse_gpui_spaceo_on_unsupported_host(cx) {
            return;
        }
        self.start_gpui_spaceo_job(gpui_spaceo_uninstall_command_action(), cx);
    }

    /// `true` (after a toast) when this computer cannot run SpaceO; Settings hides its row there.
    fn refuse_gpui_spaceo_on_unsupported_host(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        if gpui_spaceo_supported() {
            return false;
        }
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "SpaceO can't run here",
            if cfg!(target_os = "macos") {
                "SpaceO needs an Apple Silicon Mac with macOS 14 or later."
            } else {
                "SpaceO is only available on macOS."
            },
            cx,
        );
        true
    }

    pub(crate) fn check_gpui_spaceo_update(&mut self, cx: &mut gpui::Context<Self>) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let payload =
                background
                    .spawn(async move {
                        gpui_ghostex_cli_status_message_with_cua_update_check(None, true)
                    })
                    .await;
            let (level, title, message) = gpui_spaceo_update_check_toast(&payload);
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(payload.clone(), cx);
                this.dispatch_gpui_titlebar_tips_sidebar_state_payload(&payload, cx);
                this.dispatch_gpui_app_modal_toast(level, title, &message, cx);
            });
        })
        .detach();
    }

    /// Installs and updates finish by installing the Ghostex SpaceO skill; uninstalls report
    /// their result.
    fn start_gpui_spaceo_job(
        &mut self,
        action: GpuiInstallJobAction,
        cx: &mut gpui::Context<Self>,
    ) {
        self.start_gpui_install_job(
            &SPACEO_JOB,
            action,
            |operation, succeeded| match operation {
                "uninstall" => GpuiGhostexCliSettingsAction::FinishSpaceoUninstall { succeeded },
                _ => GpuiGhostexCliSettingsAction::FinishSpaceoSetup {
                    installed: succeeded,
                    was_update: operation == "update",
                },
            },
            cx,
        );
    }
}
