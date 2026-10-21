//! Agents and command terminal mount slot focus, zmx persistence refreshes and mouse forwarding.

use std::time::Duration;

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn focus_agents_terminal_mount_slot(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-06-22-23:11:
        Running terminal body clicks are focus handoffs, not placeholder activation. Keep the existing body div as the click/drop owner, update the shell focus to the clicked Agents pane, and force one AppKit first-responder handoff for the mounted real surface without adding overlays, hit-test routing, synthetic input routing, process lifecycle, or persistence fields.
        */
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }

        let gpui_engine_view = self
            .agents_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| record.view.clone());
        if let Some(view) = gpui_engine_view.as_ref() {
            // Reclaim AppKit and clear explicit CEF ownership before sidebar
            // selection notifications can run renderer work during this same
            // mouse event. The terminal FocusHandle remains the final GPUI
            // keyboard target throughout the model updates below.
            self.focus_gpui_engine_terminal_view(
                GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                view,
                window,
                cx,
            );
        }

        let workspace_focus_changed = self.agents_workspace.focused_pane != slot_id.pane_id;
        self.agents_workspace.focus_pane(slot_id.pane_id);
        self.dispatch_gpui_workspace_session_attention_acknowledge(slot_id.session_id, cx);
        self.dispatch_gpui_workspace_active_session_selected(slot_id.pane_id, cx);
        let attention_acknowledged = self
            .agents_workspace
            .acknowledge_attention_for_session_activation(slot_id.session_id);
        let focus = ShellFocusTarget::AgentsPane(slot_id.pane_id);
        let shell_focus_changed = self.shell_focus != focus;
        self.focus_shell_target_now(focus, window, cx);
        if workspace_focus_changed || shell_focus_changed || attention_acknowledged {
            self.scroll_workspace_pane_active_tab(slot_id.pane_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    /*
    CDXC:Zmx 2026-07-06:
    Clicking terminal content, including an already-focused pane, is an explicit
    opportunity to recover from a zmx daemon grid that another client changed.
    Mirrors macOS `refreshZmxPersistenceTerminalIfNeeded(mode: .ifStale)`: use
    zmx's conditional grid-size refresh for click-originated requests only, so a
    normal click inside an already-correct pane never repaints the terminal or
    scrolls it to the visible bottom.
    */
    pub(crate) fn refresh_zmx_persistence_agents_terminal_if_stale(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &gpui::Context<Self>,
    ) {
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }
        // A parked or just-surfacing engine grid is the zmx resting width,
        // not a displayed size (CDXC:Terminal 2026-09-03).
        if !self.agents_gpui_engine_terminal_zmx_grid_is_displayed(slot_id.session_id, cx) {
            return;
        }
        let session_name = self
            .agents_workspace
            .session(slot_id.session_id)
            .and_then(|session| session.zmx_session_name.clone());
        gpui_spawn_zmx_refresh_if_stale_process(
            session_name,
            self.agents_terminal_refresh_grid_size(slot_id, cx),
            "agentsTerminalContentMouseDown",
        );
    }

    pub(crate) fn refresh_zmx_persistence_command_terminal_if_stale(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
        cx: &gpui::Context<Self>,
    ) {
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }
        let session_name = self
            .command_pane
            .session(slot_id.session_id)
            .and_then(|session| session.zmx_session_name.clone());
        let grid_size = self
            .command_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| {
                let (columns, rows) = record.view.read(cx).model().size();
                (rows, columns)
            });
        gpui_spawn_zmx_refresh_if_stale_process(
            session_name,
            grid_size,
            "commandTerminalContentMouseDown",
        );
    }

    pub(crate) fn agents_terminal_refresh_grid_size(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &gpui::Context<Self>,
    ) -> Option<(u16, u16)> {
        self.agents_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| {
                let (columns, rows) = record.view.read(cx).model().size();
                (rows, columns)
            })
    }

    /*
    CDXC:Zmx 2026-07-06:
    Mirrors macOS `scheduleZmxPersistenceTerminalRefreshAfterResize`: split,
    sidebar, companion-ratio, and window resizes re-arm a trailing-edge 0.8s
    debounce, and the settled pass refreshes every surfaced zmx pane. Unlike
    macOS the settled pass stays size-gated through `refresh-if-stale` instead
    of the unconditional repaint sequence, so a pane whose grid already matches
    never repaints or scrolls.
    */
    pub(crate) fn schedule_zmx_persistence_refresh_after_resize(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        self.zmx_persistence_resize_refresh_generation = self
            .zmx_persistence_resize_refresh_generation
            .wrapping_add(1);
        let generation = self.zmx_persistence_resize_refresh_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(800))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.zmx_persistence_resize_refresh_generation != generation {
                    return;
                }
                this.refresh_zmx_persistence_surfaced_terminals_if_stale(cx);
            });
        })
        .detach();
    }

    /*
    CDXC:Zmx 2026-07-06:
    Mirrors macOS `zmxPersistenceTerminalSessionIdsForSurfacedPanes`: the visible Agents
    mount slots refresh, and rendered command-pane slots always join. Hidden panes must
    never refresh because `refresh-if-stale` conforms the daemon grid to the passed size,
    and a hidden pane's stale size would fight the surfaced owner.
    */
    pub(crate) fn refresh_zmx_persistence_surfaced_terminals_if_stale(
        &self,
        cx: &gpui::Context<Self>,
    ) {
        if self.agents_workspace_visible() {
            for slot_id in self.agents_workspace.rendered_terminal_body_mount_slots() {
                self.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx);
            }
        }
        for slot_id in self.command_pane.rendered_terminal_body_mount_slots() {
            self.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx);
        }
    }

    pub(crate) fn current_zmx_persistence_focused_terminal_slot(
        &self,
    ) -> Option<ZmxPersistenceFocusedTerminalSlot> {
        match self.shell_focus {
            ShellFocusTarget::AgentsPane(pane_id) => {
                if !self.agents_workspace_visible() {
                    return None;
                }
                let session_id = self.agents_workspace.active_session_in_pane(pane_id)?;
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id,
                };
                self.agents_workspace
                    .is_current_terminal_body_mount_slot(slot_id)
                    .then_some(ZmxPersistenceFocusedTerminalSlot::Agents(slot_id))
            }
            ShellFocusTarget::CommandPane => {
                let (group_id, session_id) = self.command_pane.focused_group_active_session_id()?;
                let slot_id = CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                };
                self.command_pane
                    .is_current_terminal_body_mount_slot(slot_id)
                    .then_some(ZmxPersistenceFocusedTerminalSlot::Command(slot_id))
            }
            _ => None,
        }
    }

    /*
    CDXC:Zmx 2026-07-06:
    Mirrors macOS `refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged` for
    non-click focus movement (keyboard pane navigation, sidebar focus routing,
    programmatic focus). Render-start change detection covers every focus call
    site without threading the refresh through each one; the conditional
    refresh makes redundant firing after click focus a size-matched no-op.
    */
    pub(crate) fn refresh_zmx_persistence_focused_terminal_if_changed(
        &mut self,
        cx: &gpui::Context<Self>,
    ) {
        let focused = self.current_zmx_persistence_focused_terminal_slot();
        if self.zmx_persistence_last_focused_terminal_slot == focused {
            return;
        }
        self.zmx_persistence_last_focused_terminal_slot = focused;
        match focused {
            Some(ZmxPersistenceFocusedTerminalSlot::Agents(slot_id)) => {
                self.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx);
            }
            Some(ZmxPersistenceFocusedTerminalSlot::Command(slot_id)) => {
                self.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx);
            }
            None => {}
        }
    }

    pub(crate) fn focus_command_terminal_mount_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Terminal 2026-06-23-09:41:
        Mounted command terminal body clicks are command-pane focus handoffs before input forwarding. Accept only the current command body mount slot, focus that command group, keep shell focus on `CommandPane`, force the existing AppKit terminal handoff path, persist the same shell focus state as placeholder clicks, and avoid new persisted fields or hit-test routing.

        CDXC:FocusRouting 2026-06-26-00:00:
        Mounted command terminal body focus must reveal the active command tab in both the expanded group strip and collapsed strip, matching native `focusTerminal(...)->revealActivePaneTab` while leaving mouse forwarding and terminal handoff ownership on the body element.
        */
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }

        if !self.command_pane.focus_group(slot_id.group_id) {
            return;
        }
        let attention_acknowledged = self
            .command_pane
            .acknowledge_attention_for_session_activation(slot_id.session_id);
        let gpui_engine_view = self
            .command_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| record.view.clone());
        if let Some(view) = gpui_engine_view.as_ref() {
            self.focus_gpui_engine_terminal_view(
                GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                view,
                window,
                cx,
            );
        }
        self.remember_current_non_command_focus();
        self.focus_shell_target_now(ShellFocusTarget::CommandPane, window, cx);
        self.scroll_command_group_active_tab(slot_id.group_id);
        self.persist_shell_layout_state();
        if attention_acknowledged {
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        }
        cx.notify();
    }
}
