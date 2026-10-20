//! Opening the command pane, its control actions, and the command pane resize and side dividers.

use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::Window;

use crate::app::consts::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn open_command_pane_from_shared_settings(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(CommandPaneGroupId, CommandSessionId, bool)> {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let result = self.command_pane.open_with_default_height_px(
            command_pane_content_height(window),
            command_pane_default_height_px_from_shared_settings(&settings_snapshot),
        );
        if let Some((group_id, session_id, true)) = result {
            self.start_command_terminal_gxserver_attach_for_slot(
                CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                },
                COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                None,
                None,
                None,
                cx,
            );
        }
        result
    }

    pub(crate) fn open_command_pane_from_command_palette(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        Open Commands Panel and F12 expand a hidden pane, focus a visible pane
        that is not already active, and minimize when the command pane already
        owns shell focus. Minimize reuses the titlebar chevron collapse path so
        sessions, height, and previous non-command focus stay intact.
        */
        // Typing in the Terminal view is command focus too, but not the Commands pane's: F12
        // must open or focus the pane then, not minimize it.
        let command_pane_focused = self.shell_focus == ShellFocusTarget::CommandPane
            && self.command_pane.focused_group_in_panel();
        match command_pane_palette_open_decision(
            self.command_pane.is_expanded(),
            command_pane_focused,
        ) {
            CommandPanePaletteOpenDecision::Minimize => {
                self.handle_command_pane_control_action(
                    CommandPaneControlAction::ToggleExpanded,
                    None,
                    window,
                    cx,
                );
            }
            CommandPanePaletteOpenDecision::OpenAndFocus
            | CommandPanePaletteOpenDecision::FocusVisible => {
                let Some((_group_id, _session_id, _created_session)) =
                    self.open_command_pane_from_shared_settings(window, cx)
                else {
                    return;
                };
                self.command_pane
                    .acknowledge_attention_for_live_focused_group_activation();
                self.focus_command_pane(cx);
                self.request_focused_command_terminal_text_focus_handoff();
                self.scroll_focused_command_active_tab();
                self.persist_shell_layout_state();
                self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn handle_command_pane_control_action(
        &mut self,
        action: CommandPaneControlAction,
        target_group_id: Option<CommandPaneGroupId>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !command_pane_focus_clicked_control_group(
            &mut self.command_pane,
            action,
            target_group_id,
        ) {
            return;
        }

        match action {
            CommandPaneControlAction::ToggleKeepOpen => {
                self.toggle_command_pane_keep_open(cx);
                return;
            }
            CommandPaneControlAction::NewCommandPlaceholder => {
                self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
                let Some((group_id, session_id)) =
                    self.command_pane.add_new_session(target_group_id)
                else {
                    return;
                };
                self.start_command_terminal_gxserver_attach_for_slot(
                    CommandTerminalBodyMountSlotId {
                        group_id,
                        session_id,
                    },
                    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                    None,
                    None,
                    None,
                    cx,
                );
                self.focus_command_pane(cx);
                self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                });
                self.scroll_focused_command_active_tab();
            }
            CommandPaneControlAction::TogglePinned => {
                let was_expanded = self.command_pane.is_expanded();
                if !was_expanded {
                    self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
                }
                self.command_pane.toggle_pinned();
                let is_expanded_after = self.command_pane.is_expanded();
                if !was_expanded
                    && is_expanded_after
                    && let Some((group_id, session_id, true)) =
                        self.command_pane.ensure_session_for_open()
                {
                    self.start_command_terminal_gxserver_attach_for_slot(
                        CommandTerminalBodyMountSlotId {
                            group_id,
                            session_id,
                        },
                        COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                        None,
                        None,
                        None,
                        cx,
                    );
                }
                if command_pane_control_action_focuses_command_pane(
                    CommandPaneControlAction::TogglePinned,
                    was_expanded,
                    is_expanded_after,
                ) {
                    self.command_pane
                        .acknowledge_attention_for_focused_session_activation();
                    self.focus_command_pane(cx);
                    self.request_focused_command_terminal_text_focus_handoff();
                    self.scroll_focused_command_active_tab();
                }
            }
            CommandPaneControlAction::ToggleExpanded => {
                let was_expanded = self.command_pane.is_expanded();
                if !was_expanded {
                    self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
                }
                self.command_pane_auto_minimize.idle_since = None;
                if was_expanded {
                    self.clear_command_pane_keep_open();
                }
                self.command_pane.toggle_expanded();
                let is_expanded_after = self.command_pane.is_expanded();
                if was_expanded && !is_expanded_after {
                    self.clear_command_resize_hover_state();
                }
                if command_pane_control_action_focuses_command_pane(
                    CommandPaneControlAction::ToggleExpanded,
                    was_expanded,
                    is_expanded_after,
                ) {
                    if let Some((group_id, session_id, true)) =
                        self.command_pane.ensure_session_for_open()
                    {
                        self.start_command_terminal_gxserver_attach_for_slot(
                            CommandTerminalBodyMountSlotId {
                                group_id,
                                session_id,
                            },
                            COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                            None,
                            None,
                            None,
                            cx,
                        );
                    }
                    self.command_pane
                        .acknowledge_attention_for_focused_session_activation();
                    self.focus_command_pane(cx);
                    self.request_focused_command_terminal_text_focus_handoff();
                    self.scroll_focused_command_active_tab();
                } else if was_expanded && !is_expanded_after {
                    self.restore_previous_non_command_focus_or_default(cx);
                }
            }
        }
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }

    pub(crate) fn handle_command_pane_empty_titlebar_mouse_down(
        &mut self,
        group_id: Option<CommandPaneGroupId>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-06-25-13:58:
        Native accepts double-click on empty pane-titlebar chrome after real tabs and controls decline the hit. GPUI command tab-strip backgrounds use this shared handler so expanded command chrome creates a New Terminal without affecting child tab/control clicks. A single click on collapsed empty chrome expands and focuses the existing active command tab.
        */
        let in_panel = group_id.is_none_or(|group_id| {
            self.command_pane.dock_for_group(group_id) != Some(CommandPaneDock::View)
        });
        if in_panel && !self.command_pane.is_expanded() {
            window.prevent_default();
            cx.stop_propagation();
            self.handle_command_pane_control_action(
                CommandPaneControlAction::ToggleExpanded,
                group_id,
                window,
                cx,
            );
            return;
        }

        if !command_pane_empty_titlebar_double_click_creates_new_terminal(event.click_count) {
            return;
        }

        window.prevent_default();
        cx.stop_propagation();
        self.handle_command_pane_control_action(
            CommandPaneControlAction::NewCommandPlaceholder,
            group_id,
            window,
            cx,
        );
    }

    pub(crate) fn open_command_pane_from_keyboard(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        F12 and the configured Open Commands Panel hotkey share the
        command-palette route: expand when hidden, focus when visible but
        inactive, minimize when the command pane is already the active surface.
        */
        self.open_command_pane_from_command_palette(window, cx);
    }

    pub(crate) fn handle_command_pane_resize_divider_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        if event.click_count >= 2 {
            self.clear_command_resize_hover_state();
            let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
            self.command_pane.reset_height_from_shared_settings(
                command_pane_content_height(window),
                &settings_snapshot,
            );
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }

        let content_height = command_pane_content_height(window);
        self.command_pane.resize_drag = Some(CommandPaneResizeDragState {
            side: GpuiCommandPaneSide::Bottom,
            start_position: event.position.y.as_f32(),
            start_extent: command_pane_height_for_ratio(
                self.command_pane.height_ratio,
                content_height,
            ),
        });
        cx.notify();
    }

    pub(crate) fn handle_command_pane_side_divider_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-08-16:
        The right-dock divider mirrors the bottom rail contract on the X axis:
        double-click resets the width ratio to the constant default, a single
        press stores the absolute start width and pointer X, and the root
        mouse-move/up handlers finish the drag through the shared resize state.
        */
        window.prevent_default();
        cx.stop_propagation();

        if event.click_count >= 2 {
            self.clear_command_resize_hover_state();
            self.command_pane.reset_width_to_default();
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }

        let content_width =
            command_pane_workspace_width(window, self.sidebar_width, self.sidebar_collapsed);
        self.command_pane.resize_drag = Some(CommandPaneResizeDragState {
            side: GpuiCommandPaneSide::Right,
            start_position: event.position.x.as_f32(),
            start_extent: command_pane_width_for_ratio(
                self.command_pane.width_ratio,
                content_width,
            ),
        });
        cx.notify();
    }

    pub(crate) fn handle_command_pane_resize_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.command_pane.resize_drag else {
            return;
        };
        if !event.dragging() {
            self.finish_command_pane_resize_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        match drag.side {
            GpuiCommandPaneSide::Bottom => {
                let next_ratio = command_pane_resize_drag_height_ratio(
                    drag,
                    event.position.y.as_f32(),
                    command_pane_content_height(window),
                );
                if (next_ratio - self.command_pane.height_ratio).abs() >= 0.001 {
                    self.command_pane.height_ratio = next_ratio;
                    cx.notify();
                }
            }
            GpuiCommandPaneSide::Right => {
                let next_ratio = command_pane_resize_drag_width_ratio(
                    drag,
                    event.position.x.as_f32(),
                    command_pane_workspace_width(
                        window,
                        self.sidebar_width,
                        self.sidebar_collapsed,
                    ),
                );
                if (next_ratio - self.command_pane.width_ratio).abs() >= 0.001 {
                    self.command_pane.width_ratio = next_ratio;
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn handle_command_pane_resize_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.command_pane.resize_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_command_pane_resize_drag(cx);
    }

    pub(crate) fn finish_command_pane_resize_drag(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:CommandPane 2026-06-25-19:13:
        Native command-panel drag continuations update layout during pointer movement, but the durable height-ratio notification is emitted once from `endCommandsPanelResize`.
        GPUI mirrors that by mutating the ratio during drag and persisting only when the stored resize state is consumed here.

        CDXC:CommandPane 2026-06-27-03:13:
        Ending top-rail resize ownership must also remove runtime hover/cursor chrome and invalidate delayed hover timers. Persist layout only for a consumed drag, but still repaint when clearing hover state is the only visible change.
        */
        let consumed_drag = self.command_pane.resize_drag.take().is_some();
        let cleared_resize_hover = self.clear_command_resize_hover_state();
        if consumed_drag {
            self.persist_shell_layout_state();
        }
        if consumed_drag || cleared_resize_hover {
            cx.notify();
        }
    }
}
