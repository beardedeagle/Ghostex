use crate::*;

pub(crate) struct CommandPaneModel {
    pub(crate) terminal_sessions: Vec<CommandTerminalSession>,
    /// The Commands pane's groups (`CommandPaneDock::Panel`).
    pub(crate) root: CommandPaneNode,
    /// The Terminal view's groups (`CommandPaneDock::View`); see `command_pane_docks.rs`.
    pub(crate) view_root: CommandPaneNode,
    /// Runtime only: whether the Terminal view is the view the panel shows. The app writes it at
    /// every mode change; it is never persisted, like `resize_drag`.
    pub(crate) view_dock_visible: bool,
    pub(crate) focused_group: CommandPaneGroupId,
    pub(crate) focus_mode_group: Option<CommandPaneGroupId>,
    pub(crate) mode: CommandPaneMode,
    pub(crate) height_ratio: f32,
    pub(crate) width_ratio: f32,
    pub(crate) resize_drag: Option<CommandPaneResizeDragState>,
    pub(crate) next_group_id: u64,
    pub(crate) next_split_id: u64,
    pub(crate) next_session_id: u64,
}

impl CommandPaneModel {
    pub(crate) fn shell_default_with_default_height_px(
        content_height: f32,
        default_height_px: f32,
    ) -> Self {
        /*
        CDXC:CommandPane 2026-06-25-11:40:
        The production GPUI command pane starts with no command terminal sessions. Opening the pane creates the first `Command Terminal` placeholder at the open boundary, while Action runs and transferred tabs can still supply specific titles. Do not seed fake Command/Shell sessions into app startup or persisted fallback state.

        CDXC:CommandPane 2026-06-27-15:00:
        The empty model still must not spawn a command terminal at startup, but GPUI now keeps the bottom command-pane strip visible so users can discover and open Commands from the workspace footer. The visible strip is presentation chrome; plus, double-click, F12, and Actions remain the only boundaries that create the first command session.
        */
        Self {
            terminal_sessions: Vec::new(),
            root: command_pane_dummy_node(),
            view_root: command_pane_dummy_node(),
            view_dock_visible: false,
            focused_group: CommandPaneGroupId(0),
            focus_mode_group: None,
            mode: CommandPaneMode::Collapsed,
            height_ratio: command_pane_default_height_ratio_for_default_height_px(
                default_height_px,
                content_height,
            ),
            width_ratio: COMMAND_PANE_DEFAULT_WIDTH_RATIO,
            resize_drag: None,
            next_group_id: 1,
            next_split_id: 1,
            next_session_id: 1,
        }
    }

    pub(crate) fn has_sessions(&self) -> bool {
        !self.terminal_sessions.is_empty()
    }

    pub(crate) fn is_expanded(&self) -> bool {
        self.mode == CommandPaneMode::Pinned
    }

    pub(crate) fn focused_group_active_session_id(
        &self,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:FocusRouting 2026-06-25-21:24:
        Native command-pane focus chrome and focused-session actions require the stored command focus and live responder to identify the same command session. GPUI shell focus is the responder proxy, so responder-style command helpers must not fall back to the first command group when focused_group is stale or missing.
        */
        self.find_leaf(self.focused_group)
            .and_then(|leaf| leaf.tab_group.active_session_id())
            .map(|session_id| (self.focused_group, session_id))
    }

    pub(crate) fn session(&self, id: CommandSessionId) -> Option<&CommandTerminalSession> {
        self.terminal_sessions
            .iter()
            .find(|session| session.id == id)
    }

    pub(crate) fn session_mut(
        &mut self,
        id: CommandSessionId,
    ) -> Option<&mut CommandTerminalSession> {
        self.terminal_sessions
            .iter_mut()
            .find(|session| session.id == id)
    }

    pub(crate) fn has_session(&self, id: CommandSessionId) -> bool {
        self.session(id).is_some()
    }

    pub(crate) fn rename_session(&mut self, id: CommandSessionId, title: String) -> bool {
        /*
        CDXC:CommandPane 2026-06-25-16:33:
        GPUI command-pane Rename Session updates only the live command-tab title. Command shell persistence remains layout/lifecycle-only and must not write user-entered titles, command text, terminal content, paths, stdout, stderr, or action payloads into shell-state JSON.

        CDXC:CommandPane 2026-06-25-22:33:
        Rename Session is a live command-tab title edit. The requested session id must still be attached to a command tab group; stale stored sessions no-op without falling back to the focused group or another tab.

        CDXC:Workarea 2026-07-04:
        Command-pane restart parity now persists this bounded display title beside the gxserver session id so renamed command tabs restore with their daemon-backed identity. The title remains chrome metadata only; command text, terminal output, paths, and attach payloads stay out of shell state.
        */
        if command_pane_group_for_session(self, id).is_none() {
            return false;
        }
        let Some(session) = self.session_mut(id) else {
            return false;
        };
        if title.is_empty() || session.title == title {
            return false;
        }
        session.title = title;
        true
    }
}
