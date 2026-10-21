//! Directional workspace focus: spatial candidates and bounds, render-order fallback and focus bounds preparation.

use gpui::Bounds;
use gpui::Pixels;
use gpui::Window;

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:FocusRouting 2026-09-20 WHY:
    /// One candidate set covers the whole workarea now that the Agents column and an open view are on
    /// screen together, so Cmd+Alt+Left and Right cross the split divider by geometry like any other
    /// pane boundary. This supersedes the 2026-07-29 rule that Left from a view restored a hidden
    /// companion: the sessions column is always there to move into.
    pub(crate) fn focus_workspace_direction(
        &mut self,
        direction: WorkspaceFocusDirection,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-07-05:
        Keyboard directional focus and placeholder body activation are separate intents. Cmd-Alt focus may select a sleeping project-editor main placeholder without waking Source, Browser, Kanban, Automate, or Docs, while explicit body activation remains the path that wakes the selected surface.
        */
        let changed = match self.focus_workspace_direction_spatial(direction, window, cx) {
            SpatialFocusOutcome::Focused => true,
            SpatialFocusOutcome::NoTarget => false,
            SpatialFocusOutcome::BoundsUnavailable => {
                self.focus_workspace_direction_by_render_order(direction, window, cx)
            }
        };

        if changed {
            cx.notify();
        }
    }

    pub(crate) fn focus_workspace_direction_spatial(
        &mut self,
        direction: WorkspaceFocusDirection,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> SpatialFocusOutcome {
        if !self.spatial_focus_bounds_ready() {
            return SpatialFocusOutcome::BoundsUnavailable;
        }

        let Some((current_target, current_bounds)) = self.current_spatial_focus_bounds() else {
            return SpatialFocusOutcome::BoundsUnavailable;
        };
        let candidates = self.spatial_focus_candidates();
        let Some(target) =
            nearest_spatial_focus_target(current_bounds, current_target, direction, &candidates)
        else {
            return SpatialFocusOutcome::NoTarget;
        };

        if self.focus_spatial_target(target, window, cx) {
            SpatialFocusOutcome::Focused
        } else {
            SpatialFocusOutcome::NoTarget
        }
    }

    /// Every pane the pointer-free directional focus can land on has to have reported bounds for this
    /// frame, or the caller falls back to render order.
    pub(crate) fn spatial_focus_bounds_ready(&self) -> bool {
        let pane_ids = self.agents_workspace.rendered_leaf_order();
        if pane_ids.is_empty()
            || !pane_ids
                .iter()
                .all(|pane_id| self.workspace_leaf_layout_bounds.contains_key(pane_id))
        {
            return false;
        }

        self.command_spatial_focus_bounds_ready()
    }

    /// CDXC:FocusRouting 2026-09-23 DECISION:
    /// User: Option+Cmd+Arrows must not go to the view panel, only between the Commands pane and the agent sessions in their split panes, so the sessions can be walked by keyboard. The view panel is not a candidate; a focus that is already inside it still moves out of it to the nearest pane in that direction.
    pub(crate) fn spatial_focus_candidates(&self) -> Vec<FocusCandidate> {
        let mut candidates = Vec::new();
        for pane_id in self.agents_workspace.rendered_leaf_order() {
            if let Some(bounds) = self.workspace_leaf_layout_bounds.get(&pane_id).copied() {
                candidates.push(FocusCandidate {
                    target: SpatialFocusTarget::AgentsPane(pane_id),
                    bounds,
                    order: candidates.len(),
                });
            }
        }

        self.append_command_spatial_focus_candidates(&mut candidates);

        candidates
    }

    /// The command groups spatial focus can move between: those in a dock that is on screen.
    pub(crate) fn visible_command_focus_group_ids(&self) -> Vec<CommandPaneGroupId> {
        self.command_pane
            .group_order()
            .into_iter()
            .filter(|group_id| self.command_pane.group_dock_visible(*group_id))
            .collect()
    }

    pub(crate) fn command_spatial_focus_bounds_ready(&self) -> bool {
        if !self.command_pane.any_dock_visible() || !self.command_pane.has_sessions() {
            return true;
        }

        let command_group_ids = self.visible_command_focus_group_ids();
        if command_group_ids.is_empty() {
            return self.command_pane_layout_bounds.is_some();
        }

        command_group_ids
            .iter()
            .all(|group_id| self.command_group_layout_bounds.contains_key(group_id))
            || self.command_pane_layout_bounds.is_some()
    }

    pub(crate) fn append_command_spatial_focus_candidates(
        &self,
        candidates: &mut Vec<FocusCandidate>,
    ) {
        if self.command_pane.any_dock_visible() && self.command_pane.has_sessions() {
            let command_group_ids = self.visible_command_focus_group_ids();
            let command_group_candidates: Option<Vec<_>> = command_group_ids
                .iter()
                .map(|group_id| {
                    self.command_group_layout_bounds
                        .get(group_id)
                        .copied()
                        .map(|bounds| (*group_id, bounds))
                })
                .collect();

            if let Some(command_group_candidates) = command_group_candidates {
                for (group_id, bounds) in command_group_candidates {
                    candidates.push(FocusCandidate {
                        target: SpatialFocusTarget::CommandPaneGroup(group_id),
                        bounds,
                        order: candidates.len(),
                    });
                }
            } else if let Some(bounds) = self.command_pane_layout_bounds {
                candidates.push(FocusCandidate {
                    target: SpatialFocusTarget::CommandPane,
                    bounds,
                    order: candidates.len(),
                });
            }
        }
    }

    pub(crate) fn current_spatial_focus_bounds(
        &self,
    ) -> Option<(SpatialFocusTarget, Bounds<Pixels>)> {
        match self.shell_focus {
            ShellFocusTarget::AgentsPane(pane_id) => self
                .workspace_leaf_layout_bounds
                .get(&pane_id)
                .copied()
                .map(|bounds| (SpatialFocusTarget::AgentsPane(pane_id), bounds)),
            ShellFocusTarget::CommandPane if self.command_pane.focused_group_dock_visible() => {
                self.current_command_spatial_focus_bounds()
            }
            ShellFocusTarget::ProjectEditorSurface(mode) if self.active_mode == mode => self
                .project_editor_surface_bounds_for_mode(mode)
                .map(|bounds| (SpatialFocusTarget::ProjectEditorSurface(mode), bounds)),
            ShellFocusTarget::BrowserPane(pane_id) if self.active_mode == TitlebarMode::Browser => {
                self.browser_leaf_layout_bounds
                    .get(&pane_id)
                    .copied()
                    .map(|bounds| (SpatialFocusTarget::BrowserPane(pane_id), bounds))
                    .or_else(|| self.browser_surface_spatial_focus_bounds())
            }
            ShellFocusTarget::BrowserSurface if self.active_mode == TitlebarMode::Browser => {
                self.browser_surface_spatial_focus_bounds()
            }
            _ => None,
        }
    }

    /// The Browser view's own bounds: its focused leaf while it is awake, else the surface slot.
    fn browser_surface_spatial_focus_bounds(&self) -> Option<(SpatialFocusTarget, Bounds<Pixels>)> {
        let mode = TitlebarMode::Browser;
        if self.project_editor_shell.is_mode_awake(mode) {
            let pane_id = self.browser_tabs.focused_pane;
            if let Some(bounds) = self.browser_leaf_layout_bounds.get(&pane_id).copied() {
                return Some((SpatialFocusTarget::BrowserPane(pane_id), bounds));
            }
        }
        self.project_editor_surface_bounds_for_mode(mode)
            .map(|bounds| (SpatialFocusTarget::ProjectEditorSurface(mode), bounds))
    }

    pub(crate) fn current_command_spatial_focus_bounds(
        &self,
    ) -> Option<(SpatialFocusTarget, Bounds<Pixels>)> {
        if !self.command_pane.focused_group_dock_visible() {
            return None;
        }

        let group_id = self.command_pane.focused_group;
        self.command_group_layout_bounds
            .get(&group_id)
            .copied()
            .map(|bounds| (SpatialFocusTarget::CommandPaneGroup(group_id), bounds))
            .or_else(|| {
                self.command_pane_layout_bounds
                    .map(|bounds| (SpatialFocusTarget::CommandPane, bounds))
            })
    }

    pub(crate) fn focus_spatial_target(
        &mut self,
        target: SpatialFocusTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match target {
            SpatialFocusTarget::AgentsPane(pane_id) => {
                self.focus_agents_pane(pane_id, cx);
                true
            }
            SpatialFocusTarget::BrowserPane(pane_id) => {
                self.focus_browser_pane(pane_id, window, cx)
            }
            SpatialFocusTarget::ProjectEditorSurface(mode) => {
                self.focus_project_editor_surface_for_keyboard(mode, window, cx)
            }
            SpatialFocusTarget::CommandPane => self.focus_command_pane_directional_target(None, cx),
            SpatialFocusTarget::CommandPaneGroup(group_id) => {
                self.focus_command_pane_directional_target(Some(group_id), cx)
            }
        }
    }

    pub(crate) fn focus_workspace_direction_by_render_order(
        &mut self,
        direction: WorkspaceFocusDirection,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let targets = self.workspace_render_order_focus_targets();
        if targets.is_empty() {
            return false;
        }

        let current_target = match self.shell_focus {
            ShellFocusTarget::CommandPane => {
                let focused_group_target =
                    SpatialFocusTarget::CommandPaneGroup(self.command_pane.focused_group);
                if targets.contains(&focused_group_target) {
                    Some(focused_group_target)
                } else if targets.contains(&SpatialFocusTarget::CommandPane) {
                    Some(SpatialFocusTarget::CommandPane)
                } else {
                    None
                }
            }
            ShellFocusTarget::AgentsPane(pane_id) => Some(SpatialFocusTarget::AgentsPane(pane_id)),
            // The view panel is not in the order (see `spatial_focus_candidates`): leave it for the
            // focused agent pane.
            _ => {
                self.focus_agents_pane(self.agents_workspace.focused_pane, cx);
                return true;
            }
        };

        let Some(current_target) = current_target else {
            self.focus_agents_pane(self.agents_workspace.focused_pane, cx);
            return true;
        };
        render_order_focus_target(&targets, Some(current_target), direction)
            .map(|target| self.focus_spatial_target(target, window, cx))
            .unwrap_or(false)
    }

    pub(crate) fn workspace_render_order_focus_targets(&self) -> Vec<SpatialFocusTarget> {
        workspace_render_order_focus_targets(
            self.agents_workspace.rendered_leaf_order(),
            self.command_pane.any_dock_visible(),
            self.command_pane.has_sessions(),
            self.visible_command_focus_group_ids(),
        )
    }

    pub(crate) fn project_editor_surface_bounds_for_mode(
        &self,
        mode: TitlebarMode,
    ) -> Option<Bounds<Pixels>> {
        self.project_editor_surface_layout_bounds
            .filter(|focus_bounds| focus_bounds.mode == mode)
            .map(|focus_bounds| focus_bounds.bounds)
    }

    pub(crate) fn prepare_focus_bounds_for_render(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:Terminal 2026-06-22-20:29:
        Agents terminal mount-slot bounds are App-owned runtime geometry only. Clear them with the other per-render focus/layout bounds without persisting, logging, or retaining stale pane/session geometry.

        CDXC:Terminal 2026-06-22-21:27:
        This per-render bounds clear is a pre-layout measurement reset, not a terminal slot removal. Host sync must preserve the focused running Agents slot while awaiting the body canvas record, and only the runtime bounds map should be empty during that interval.
        */
        self.workspace_leaf_layout_bounds.clear();
        self.browser_leaf_layout_bounds.clear();
        self.command_group_layout_bounds.clear();
        self.command_terminal_mount_slot_bounds.clear();
        self.command_pane_layout_bounds = None;
        self.project_editor_surface_layout_bounds = None;
        self.agents_terminal_mount_slot_bounds.clear();
        self.agents_terminal_startup_body_slot_geometries.clear();
        // Prune (do not clear) the persistent zmx-refresh bounds maps: a slot
        // that stops being rendered loses its entry here, so its next body
        // record counts as a fresh surfacing and triggers the conditional
        // refresh; a continuously rendered slot keeps its entry, so per-frame
        // records stay refresh-free (CDXC:Zmx 2026-07-11).
        if self.agents_workspace_visible() {
            let rendered = self.agents_workspace.rendered_terminal_body_mount_slots();
            self.agents_terminal_zmx_refresh_recorded_bounds
                .retain(|slot_id, _| rendered.contains(slot_id));
        } else {
            self.agents_terminal_zmx_refresh_recorded_bounds.clear();
        }
        {
            let rendered = self.command_pane.rendered_terminal_body_mount_slots();
            self.command_terminal_zmx_refresh_recorded_bounds
                .retain(|slot_id, _| rendered.contains(slot_id));
        }
        self.sync_agents_terminal_surface_host(cx);
        self.sync_command_terminal_surface_host(cx);
        self.sync_gpui_engine_agents_chat_eligibility(cx);
    }
}
