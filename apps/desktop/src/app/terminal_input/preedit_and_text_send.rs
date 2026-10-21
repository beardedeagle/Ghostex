//! IME preedit, sending text, Tab and Return keys and command action scripts to terminal surfaces.

use std::path::Path;
use std::sync::atomic::Ordering;

use gpui::Bounds;
use gpui::Pixels;
use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn set_preedit_on_focused_terminal_surface(&mut self, bytes: &[u8]) -> bool {
        let Some(target) = self.exact_focused_terminal_text_surface_target() else {
            return false;
        };
        self.set_preedit_on_terminal_text_target(target, bytes)
    }

    pub(crate) fn set_preedit_on_terminal_text_target(
        &mut self,
        _target: FocusedTerminalTextMountTarget,
        _bytes: &[u8],
    ) -> bool {
        false
    }

    pub(crate) fn clear_focused_terminal_preedit(&mut self) {
        let _ = self.set_preedit_on_focused_terminal_surface(b"");
        self.terminal_text_marked_range = None;
    }

    pub(crate) fn bounds_for_focused_terminal_ime_point(
        &self,
        _element_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    pub(crate) fn send_text_to_focused_terminal_surface(
        &mut self,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if text.is_empty() {
            return false;
        }

        if let Some(view) = self.focused_gpui_engine_terminal_view() {
            view.update(cx, |view, cx| view.send_text_input(text, cx));
            return true;
        }

        false
    }

    pub(crate) fn send_tab_key_to_gpui_engine_terminal(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        action: ghostty_vt::VtKeyAction,
        shift: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(view) = self.gpui_engine_terminal_view_for_target(target) else {
            return false;
        };
        view.update(cx, |view, cx| view.send_tab_key_action(action, shift, cx))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_window_scoped_ghostex_hotkey(
        &mut self,
        action_id: &str,
        owner: GpuiKeyboardOwner,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Hotkeys 2026-07-24:
        Native pre-dispatch captures the window and exact keyboard owner before
        queuing an app action. Never bounce the selector through a process-global
        callback or another window's current focus; the target app entity and
        Window supplied by update_in are the registration that accepted it.
        */
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.keyboardRouter.hotkeyDispatched",
            serde_json::json!({
                "actionId": action_id,
                "owner": format!("{owner:?}"),
            }),
        );
        self.handle_gpui_app_modal_sidebar_command(
            serde_json::json!({
                "message": {
                    "actionId": action_id,
                    "type": "runGhostexHotkeyAction",
                },
            }),
            window,
            cx,
        );
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_window_scoped_application_keyboard_command(
        &mut self,
        command: GpuiApplicationKeyboardCommand,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            GpuiApplicationKeyboardCommand::Hide => cx.hide(),
            GpuiApplicationKeyboardCommand::HideOthers => cx.hide_other_apps(),
            GpuiApplicationKeyboardCommand::MinimizeWindow => window.minimize_window(),
            GpuiApplicationKeyboardCommand::Quit => {
                GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
                cx.quit();
            }
        }
    }

    pub(crate) fn send_return_key_to_mounted_command_terminal_surface(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:DelayedSend 2026-06-25-15:11:
        Delayed Send must submit the staged prompt through Ghostty's key path, matching native `sendTerminalEnter`, rather than writing carriage-return text. Use the exact current mounted command slot and the macOS Return keycode/text tuple; if the surface is missing or stale, no other terminal receives the key.
        */
        if self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
            && let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id)
        {
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_return_key(cx));
            return true;
        }
        false
    }

    pub(crate) fn send_return_key_to_mounted_agents_terminal_surface(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:SessionTitles 2026-06-27-02:27:
        Mapped Agents rename submission must use Ghostty's key path for the exact mounted Agents slot, matching native Return delivery. If the slot is stale, hidden, sleeping, missing a runtime id, or owned by a different surface, no other terminal receives a newline or fallback key.
        */
        if self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
            && let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
        {
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_return_key(cx));
            return true;
        }
        false
    }

    pub(crate) fn send_gpui_command_action_script_to_mounted_terminal(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        execution_text: &str,
        status_file_path: &Path,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        #[cfg(target_os = "windows")]
        {
            let input = if matches!(
                windows_terminal_backend::resolve_current(),
                Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
            ) {
                execution_text.to_string()
            } else {
                gpui_command_action_mounted_terminal_script_text(execution_text, status_file_path)
            };
            let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id) else {
                return false;
            };
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_text_input(&input, cx));
            return self.send_return_key_to_mounted_command_terminal_surface(slot_id, cx);
        }
        #[cfg(not(target_os = "windows"))]
        let Some(source_command) = gpui_command_action_staged_mounted_script_source_command(
            execution_text,
            status_file_path,
        ) else {
            return false;
        };
        #[cfg(not(target_os = "windows"))]
        if let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id) {
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_text_input(&source_command, cx));
            return self.send_return_key_to_mounted_command_terminal_surface(slot_id, cx);
        }

        false
    }

    pub(crate) fn gpui_command_action_mounted_reuse_surface_available(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-27-07:54:
        Default Action reuse may bypass launch payloads only when the selected reused tab already owns the exact current mounted command terminal. Missing, stale, sleeping, or unmounted reused tabs must use the exact-slot launch payload path instead of borrowing another terminal surface.
        */
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        self.command_gpui_engine_terminals
            .contains_key(&slot_id.session_id)
    }
}
