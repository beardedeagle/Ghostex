//! What a capture does when macOS has not given Ghostex Screen Recording yet.
//!
//! CDXC:GhostexCapture 2026-10-16 WHY:
//! A Discord user reported every Capture button crashing Ghostex on their Mac. The permission
//! request (`CGRequestScreenCaptureAccess`) ran inside the click's app update, so anything macOS
//! delivered while it showed its prompt reached GPUI with the app already borrowed. It now runs
//! from a task of its own, after the click's update has ended. macOS shows its own prompt only
//! the first time an app asks, so a later click opens the Screen Recording pane instead, where
//! the user can turn Ghostex on. A grant made while Ghostex runs takes effect only after a
//! relaunch on older macOS, which the message says.

use gpui::Context;

use super::grab;
use crate::GhostexGpuiApp;
use crate::app::helpers::GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL;

impl GhostexGpuiApp {
    pub(super) fn ask_ghostex_capture_screen_access(&mut self, cx: &mut Context<Self>) {
        let asked_before = self.ghostex_capture.screen_access_asked;
        self.ghostex_capture.screen_access_asked = true;
        cx.spawn(async move |this, cx| {
            if !asked_before {
                grab::screen_access(true);
            }
            let _ = this.update(cx, |app, cx| {
                if asked_before {
                    app.open_gpui_macos_system_settings_url(
                        GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL,
                        "openScreenRecordingPreferences",
                        cx,
                    );
                }
                app.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Allow Screen Recording",
                    "Ghostex needs Screen Recording to take screenshots. Turn Ghostex on in System Settings > Privacy & Security > Screen Recording. If it is already on, quit and reopen Ghostex.",
                    cx,
                );
            });
        })
        .detach();
    }
}
