// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds macOS notification permission/test-notification/completion-sound preview plus the after-save settings sync handlers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, keep-awake

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn request_gpui_macos_notification_permission(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Notifications 2026-06-24-12:44:
        The GPUI Settings notification permission action mirrors the native Settings boundary: read macOS authorization, request only alert permission when it is notDetermined, report denied as a system-settings repair path, and never fake success with an in-app notification fallback.
        */
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let status = background
                .spawn(async move { gpui_request_macos_notification_permission() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(
                    gpui_notification_permission_status_message(status),
                    cx,
                );
                this.dispatch_gpui_settings_action_status(
                    "requestMacOSNotificationPermission",
                    status.available(),
                    status.message(),
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    status.toast_level(),
                    status.toast_title(),
                    status.message(),
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn deliver_gpui_macos_settings_test_notification(
        &mut self,
        completion_sound_enabled: bool,
        played_sound: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_deliver_macos_settings_test_notification() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(status) = result.permission_status() {
                    this.dispatch_open_gpui_app_modal_sidebar_state_payload(
                        gpui_notification_permission_status_message(status),
                        cx,
                    );
                }
                let (message, available) = gpui_macos_notification_test_action_message(
                    result,
                    completion_sound_enabled,
                    played_sound,
                );
                this.dispatch_gpui_settings_action_status(
                    "testAgentTaskCompletion",
                    available,
                    message,
                    cx,
                );
                if result == GpuiMacOSNotificationDeliveryResult::Sent {
                    this.dispatch_gpui_app_modal_toast(
                        "success",
                        "Test notification sent",
                        message,
                        cx,
                    );
                } else {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Test notification unavailable",
                        message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn play_gpui_completion_sound_preview(
        &mut self,
        sound: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let normalized_sound = gpui_normalize_completion_sound(sound);
        match gpui_play_completion_sound(normalized_sound) {
            Ok(()) => true,
            Err(message) => {
                self.dispatch_open_gpui_app_modal_sidebar_state_payload(
                    gpui_sound_preview_status_message(false, &message),
                    cx,
                );
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Sound preview unavailable",
                    &message,
                    cx,
                );
                false
            }
        }
    }

    pub(crate) fn test_gpui_agent_task_completion(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:Notifications 2026-06-24-12:44:
        Settings test-agent-completion parity is intentionally bounded to the local test action. Continue the existing completion-sound preview when enabled, then send one generic no-sound macOS banner only when the Settings notification toggle is enabled; do not route real session attention notifications, names, paths, command text, URLs, or click-to-focus metadata through this path.
        */
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings = settings_snapshot.object();
        let completion_sound = json_string_field(settings, "completionSound");
        let completion_sound_enabled = completion_sound != Some("off");
        let notifications_enabled =
            json_bool_field(settings, "showMacOSAttentionNotifications").unwrap_or(true);
        let mut played_sound = false;
        if completion_sound_enabled {
            played_sound = self.play_gpui_completion_sound_preview(completion_sound, cx);
        }

        if notifications_enabled {
            self.deliver_gpui_macos_settings_test_notification(
                completion_sound_enabled,
                played_sound,
                cx,
            );
            return;
        }

        if completion_sound_enabled {
            let (available, message) = if played_sound {
                (
                    true,
                    "Played the configured completion sound. Attention notifications are disabled in Settings.",
                )
            } else {
                (
                    false,
                    "The completion sound preview failed and attention notifications are disabled in Settings.",
                )
            };
            self.dispatch_gpui_settings_action_status(
                "testAgentTaskCompletion",
                available,
                message,
                cx,
            );
        } else if !played_sound {
            let message =
                "Current Settings have completion sounds and attention notifications disabled.";
            self.dispatch_gpui_settings_action_status(
                "testAgentTaskCompletion",
                false,
                message,
                cx,
            );
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Completion alerts are disabled",
                message,
                cx,
            );
        }
    }

    pub(crate) fn refresh_open_gpui_app_modal_sidebar_state_in_background(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let sidebar_state_message = background
                .spawn(async move {
                    gpui_app_modal_sidebar_state_message_for_active_project_id(
                        active_project_id.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
            });
        })
        .detach();
    }

    pub(crate) fn sync_gpui_removed_remote_machine_passwords_after_settings_save(
        &mut self,
        previous_settings: &serde_json::Map<String, serde_json::Value>,
        next_settings: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-06-24-13:36:
        Normal GPUI Settings saves mirror native-sidebar cleanup for deleted Remote Machines: after the settings object is already saved, remove Keychain passwords for disappeared machines that carried `sshPasswordSaved === true`. This cleanup is silent and best-effort, carries only bounded ids, and never blocks the settings save or logs machine/user/host/password details.
        */
        let removed_ids =
            gpui_removed_remote_machine_password_ids(previous_settings, next_settings);
        if removed_ids.is_empty() {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, _cx| {
            background
                .spawn(async move {
                    for remote_machine_id in removed_ids {
                        let _ =
                            gpui_save_remote_machine_password_to_keychain(&remote_machine_id, "");
                    }
                })
                .await;
        })
        .detach();
    }

    pub(crate) fn update_gpui_project_settings_metadata_in_background(
        &mut self,
        update: GpuiProjectSettingsMetadataUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Projects 2026-06-24-11:59:
        GPUI Settings project metadata edits are gxserver-owned. Apply worktree command, Beads display-key, and Beads directory changes through `/api/updateProject`, then rehydrate the open modal from real gxserver/shared Settings state instead of mutating local fake project rows or shelling out.
        */
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let sidebar_state_message = background
                .spawn(async move {
                    let _ = gpui_update_project_settings_metadata(update);
                    gpui_app_modal_sidebar_state_message_for_active_project_id(
                        active_project_id.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
            });
        })
        .detach();
    }
}
