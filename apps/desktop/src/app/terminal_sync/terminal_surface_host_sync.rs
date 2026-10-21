// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: layout-bounds bookkeeping for terminal mount slots plus command terminal surface host sync.

use std::sync::atomic::Ordering;

use gpui::Bounds;
use gpui::Pixels;

use crate::app::consts::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn record_workspace_leaf_layout_bounds(
        &mut self,
        pane_id: WorkspacePaneId,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.workspace_leaf_layout_bounds.insert(pane_id, bounds);
        }
    }

    pub(crate) fn record_agents_terminal_mount_slot_bounds(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        bounds: Bounds<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Terminal 2026-06-22-22:45:
        Store exact terminal body bounds only after validating the pane/session is still one of the current rendered running mount slots. This record is runtime-only and must not be serialized, logged, or represented by a hidden hitbox; the existing body div remains the click/drop owner.
        */
        let current_slot_ids = self.agents_workspace.rendered_terminal_body_mount_slots();
        let slot_is_current = self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id);
        // Surfaced/resize detection reads the render-persistent refresh map,
        // not the per-render-cleared geometry map above — the geometry map is
        // always empty at first record of a frame, which made every frame
        // look freshly surfaced (one zmx subprocess per slot per frame) and
        // left the resize-debounce arm unreachable
        // (CDXC:Zmx 2026-07-11).
        let previous_bounds = self
            .agents_terminal_zmx_refresh_recorded_bounds
            .get(&slot_id)
            .copied();
        self.agents_terminal_mount_slot_bounds
            .retain(|stored_slot_id, _| current_slot_ids.contains(stored_slot_id));
        if slot_is_current {
            self.agents_terminal_mount_slot_bounds
                .insert(slot_id, bounds);
            self.agents_terminal_zmx_refresh_recorded_bounds
                .insert(slot_id, bounds);
        } else {
            self.agents_terminal_mount_slot_bounds.remove(&slot_id);
            self.agents_terminal_zmx_refresh_recorded_bounds
                .remove(&slot_id);
        }
        self.sync_agents_terminal_surface_host(cx);
        /*
        CDXC:Zmx 2026-07-06:
        A slot recording bounds for the first time was just surfaced (tab
        switch, mode switch, wake, restore) and may face a zmx daemon grid
        another client resized while it was hidden — refresh it conditionally
        after host sync so a reattached surface reports its real size. A
        same-slot body size change instead re-arms the trailing-edge resize
        debounce, mirroring macOS surfaced-pane and resize-settled refreshes.
        */
        if self.agents_workspace_visible() && slot_is_current {
            match previous_bounds {
                None => self.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx),
                Some(previous) if previous.size != bounds.size => {
                    self.schedule_zmx_persistence_refresh_after_resize(cx);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn record_agents_terminal_startup_body_slot_bounds(
        &mut self,
        slot_id: AgentsTerminalStartupBodySlotId,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Terminal 2026-06-23-00:10:
        Record Mounting startup body geometry only for the current visible selected Mounting slot and keep it keyed by `AgentsTerminalStartupBodySlotId`, not the Running mount slot. The existing placeholder body remains the click/drop owner, and this runtime map is cleared or pruned before it can become stale, persisted, logged, or used to create a Ghostty surface.
        */
        record_agents_terminal_startup_body_slot_geometry(
            self.agents_workspace_visible(),
            &self.agents_workspace,
            &mut self.agents_terminal_startup_body_slot_geometries,
            slot_id,
            bounds,
            scale_factor,
        );
        self.sync_agents_terminal_surface_host(cx);
    }

    pub(crate) fn record_command_terminal_mount_slot_bounds(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        bounds: Bounds<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Terminal 2026-06-23-05:03:
        Command terminal bounds are recorded from the command body element itself, never from the enclosing group or titlebar. The record is accepted only for a current expanded visible active command session and stays runtime-only so shell-state JSON, command titles, paths, commands, env, output, and terminal content cannot leak into launch payload or persistence.
        */
        let current_slot_ids = self.command_pane.rendered_terminal_body_mount_slots();
        let slot_is_current = self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id);
        // Render-persistent refresh map, not the per-render-cleared geometry
        // map — see the Agents bounds hook
        // (CDXC:Zmx 2026-07-11).
        let previous_bounds = self
            .command_terminal_zmx_refresh_recorded_bounds
            .get(&slot_id)
            .copied();
        self.command_terminal_mount_slot_bounds
            .retain(|stored_slot_id, _| current_slot_ids.contains(stored_slot_id));
        if slot_is_current {
            self.command_terminal_mount_slot_bounds
                .insert(slot_id, bounds);
            self.command_terminal_zmx_refresh_recorded_bounds
                .insert(slot_id, bounds);
        } else {
            self.command_terminal_mount_slot_bounds.remove(&slot_id);
            self.command_terminal_zmx_refresh_recorded_bounds
                .remove(&slot_id);
        }
        self.sync_command_terminal_surface_host(cx);
        // See the Agents bounds hook: first-record = surfaced refresh, body
        // size change = trailing-edge resize debounce (CDXC:Zmx).
        if slot_is_current {
            match previous_bounds {
                None => self.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx),
                Some(previous) if previous.size != bounds.size => {
                    self.schedule_zmx_persistence_refresh_after_resize(cx);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn sync_command_terminal_surface_host(&mut self, cx: &mut gpui::Context<Self>) {
        if GPUI_APP_QUIT_IN_PROGRESS.load(Ordering::Acquire) || self.workspace_window_closing {
            return;
        }
        self.sync_command_gpui_engine_terminals(cx);
        self.command_terminal_mount_slot_bounds.clear();
        self.sync_command_pane_view_dock_visibility();
    }
}
