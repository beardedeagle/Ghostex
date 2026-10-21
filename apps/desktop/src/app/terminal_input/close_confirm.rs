//! Terminal close-confirm dialogs for agents and command terminals.

use gpui::Window;
use gpui_component::WindowExt;
use gpui_component::button::ButtonVariant;
use gpui_component::dialog::DialogButtonProps;

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn agents_terminal_close_confirm_slot_is_current(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.agents_gpui_engine_close_confirms.contains(&slot_id)
            && self
                .agents_gpui_engine_terminals
                .contains_key(&slot_id.session_id)
            && self
                .agents_workspace
                .is_current_terminal_body_mount_slot(slot_id)
            && self
                .agents_workspace
                .can_close_tab(slot_id.pane_id, slot_id.session_id)
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn command_terminal_close_confirm_slot_is_current(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        self.command_gpui_engine_close_confirms.contains(&slot_id)
            && self
                .command_gpui_engine_terminals
                .contains_key(&slot_id.session_id)
            && self
                .command_pane
                .is_current_terminal_body_mount_slot(slot_id)
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn terminal_close_confirm_dialog_key_is_current(
        &self,
        key: TerminalCloseConfirmDialogKey,
    ) -> bool {
        match key {
            TerminalCloseConfirmDialogKey::Agents(slot_id) => {
                self.agents_terminal_close_confirm_slot_is_current(slot_id)
            }
            TerminalCloseConfirmDialogKey::Command(slot_id) => {
                self.command_terminal_close_confirm_slot_is_current(slot_id)
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn next_terminal_close_confirm_dialog_key(
        &self,
    ) -> Option<TerminalCloseConfirmDialogKey> {
        if let Some(slot_id) =
            focused_command_terminal_surface_mount_slot(self.shell_focus, &self.command_pane)
                .filter(|slot_id| self.command_terminal_close_confirm_slot_is_current(*slot_id))
        {
            return Some(TerminalCloseConfirmDialogKey::Command(slot_id));
        }
        if let Some(slot_id) = focused_agents_terminal_surface_mount_slot(
            self.active_mode,
            self.shell_focus,
            &self.agents_workspace,
        )
        .filter(|slot_id| self.agents_terminal_close_confirm_slot_is_current(*slot_id))
        {
            return Some(TerminalCloseConfirmDialogKey::Agents(slot_id));
        }
        if let Some(slot_id) = self
            .command_pane
            .rendered_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| self.command_terminal_close_confirm_slot_is_current(*slot_id))
        {
            return Some(TerminalCloseConfirmDialogKey::Command(slot_id));
        }
        self.agents_workspace
            .rendered_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| self.agents_terminal_close_confirm_slot_is_current(*slot_id))
            .map(TerminalCloseConfirmDialogKey::Agents)
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn sync_terminal_close_confirm_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(key) = self.terminal_close_confirm_dialog_key {
            if self.terminal_close_confirm_dialog_key_is_current(key) {
                return;
            }
            self.terminal_close_confirm_dialog_key = None;
            if window.has_active_dialog(cx) {
                window.close_dialog(cx);
            }
        }

        if window.has_active_dialog(cx) {
            return;
        }
        let Some(key) = self.next_terminal_close_confirm_dialog_key() else {
            return;
        };
        self.terminal_close_confirm_dialog_key = Some(key);
        cx.defer_in(window, move |this, window, cx| {
            if this.terminal_close_confirm_dialog_key != Some(key)
                || !this.terminal_close_confirm_dialog_key_is_current(key)
            {
                this.terminal_close_confirm_dialog_key = None;
                return;
            }
            if window.has_active_dialog(cx) {
                this.terminal_close_confirm_dialog_key = None;
                cx.notify();
                return;
            }
            this.open_terminal_close_confirm_dialog(key, window, cx);
        });
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn open_terminal_close_confirm_dialog(
        &mut self,
        key: TerminalCloseConfirmDialogKey,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let signature = terminal_close_confirm_surface_signature(key.family());
        let entity = cx.entity().clone();
        let ok_entity = entity.clone();
        let cancel_entity = entity;

        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title(signature.title)
                .description(signature.message)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text(signature.keep_open_label)
                        .ok_text(signature.confirm_action_label)
                        .ok_variant(ButtonVariant::Default)
                        .on_ok({
                            let ok_entity = ok_entity.clone();
                            move |_, _, cx| {
                                ok_entity.update(cx, |this, cx| {
                                    if !this.terminal_close_confirm_dialog_key_is_current(key) {
                                        this.terminal_close_confirm_dialog_key = None;
                                        return true;
                                    }
                                    let confirmed = match key {
                                        TerminalCloseConfirmDialogKey::Agents(slot_id) => {
                                            this.confirm_pending_agents_terminal_close(slot_id, cx)
                                        }
                                        TerminalCloseConfirmDialogKey::Command(slot_id) => {
                                            this.confirm_pending_command_terminal_close(slot_id, cx)
                                        }
                                    };
                                    if confirmed
                                        && !this.terminal_close_confirm_dialog_key_is_current(key)
                                    {
                                        this.terminal_close_confirm_dialog_key = None;
                                    }
                                    if confirmed {
                                        cx.notify();
                                    }
                                    confirmed
                                })
                            }
                        })
                        .on_cancel({
                            let cancel_entity = cancel_entity.clone();
                            move |_, _, cx| {
                                cancel_entity.update(cx, |this, cx| {
                                    let canceled = match key {
                                        TerminalCloseConfirmDialogKey::Agents(slot_id) => {
                                            this.cancel_pending_agents_terminal_close(slot_id)
                                        }
                                        TerminalCloseConfirmDialogKey::Command(slot_id) => {
                                            this.cancel_pending_command_terminal_close(slot_id)
                                        }
                                    };
                                    this.terminal_close_confirm_dialog_key = None;
                                    if canceled {
                                        cx.notify();
                                    }
                                    true
                                })
                            }
                        }),
                )
                .overlay_closable(false)
                .close_button(false)
                .keyboard(true)
        });
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn confirm_pending_agents_terminal_close(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-23-20:04:
        Direct user confirmation has the same shell side effects as the callback close path: reconcile process-local runtime ids after model removal, keep Agents focus on the surviving focused pane, scroll its active tab, and persist layout state without touching command/startup maps.
        */
        if self
            .local_workspace_key_for_shell_session(slot_id.session_id)
            .is_some()
        {
            return false;
        }
        if !self.agents_gpui_engine_close_confirms.remove(&slot_id) {
            return false;
        }
        self.agents_gpui_engine_terminals
            .remove(&slot_id.session_id);
        let confirmed = self
            .agents_workspace
            .close_tab(slot_id.pane_id, slot_id.session_id);
        if confirmed {
            self.forget_local_workspace_mappings_for_shell_session(slot_id.session_id, cx);
        }
        if confirmed {
            self.agents_terminal_runtime_sessions
                .reconcile_with_workspace(&self.agents_workspace);
            self.focus_shell_target(
                ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
                cx,
            );
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
        }
        confirmed
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn cancel_pending_agents_terminal_close(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.agents_gpui_engine_close_confirms.remove(&slot_id)
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn confirm_pending_command_terminal_close(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-23-20:04:
        Command confirmation mirrors the confirmed-callback shell side effects: if sessions remain, keep focus on the command pane and scroll the active command tab; if the pane empties, restore the previous non-command focus before persisting layout state.

        CDXC:CommandPane 2026-06-25-21:12:
        Direct user confirmation must also share the command close cleanup side effects from callback and tab-close paths. Prune command-owned Delayed Send and Close After Done timers, refresh the sidebar command projection, and repaint immediately after the confirmed command session leaves the model.

        CDXC:CommandPane 2026-06-27-03:21:
        Direct confirmation can remove the final mounted command tab from the close-confirm surface. Clear command resize hover chrome only after that leaves the command pane empty, matching runtime confirmed-close and process-exit cleanup.
        */
        if !self.command_gpui_engine_close_confirms.remove(&slot_id) {
            return false;
        }
        self.command_gpui_engine_terminals
            .remove(&slot_id.session_id);
        let confirmed = self
            .command_pane
            .close_session(slot_id.group_id, slot_id.session_id);
        if confirmed {
            self.forget_command_gxserver_session_for_closed_tab(slot_id.session_id, cx);
            self.prune_gpui_command_delayed_send_timers_for_command_model();
            self.prune_gpui_command_close_after_done_timers_for_command_model();
            self.clear_command_resize_hover_state_if_command_pane_hidden();
            if self.command_pane.has_sessions() {
                self.focus_shell_target(ShellFocusTarget::CommandPane, cx);
                self.scroll_focused_command_active_tab();
            } else {
                self.restore_previous_non_command_focus_or_default(cx);
            }
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
        }
        confirmed
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn cancel_pending_command_terminal_close(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        self.command_gpui_engine_close_confirms.remove(&slot_id)
    }
}
