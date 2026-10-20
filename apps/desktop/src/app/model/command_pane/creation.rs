use super::*;
use crate::*;

impl CommandPaneModel {
    pub(crate) fn prepare_hidden_open_with_default_height_px(
        &mut self,
        content_height: f32,
        default_height_px: f32,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-25-11:47:
        Opening a hidden GPUI command pane must match macOS `createCommandsPanelOpenStatePatch`: reset height from the Workspace default only when the pane is hidden, and preserve the user's live resize while the pane is already expanded. Keep this model-local so F12, titlebar Actions, sidebar Actions, and command chrome share the same rule.

        CDXC:CommandPane 2026-08-16:
        Only the height resets here, because it comes from the Workspace default-height Setting. The right dock's width has no Settings default, so its ratio is user-owned: opening the pane again keeps the width the divider drag stored, and the divider's double-click reset stays the only way back to the default.
        */
        if self.is_expanded() {
            return false;
        }

        self.reset_height_with_default_height_px(content_height, default_height_px);
        true
    }

    pub(crate) fn open_with_default_height_px(
        &mut self,
        content_height: f32,
        default_height_px: f32,
    ) -> Option<(CommandPaneGroupId, CommandSessionId, bool)> {
        self.prepare_hidden_open_with_default_height_px(content_height, default_height_px);
        self.ensure_session_for_open()
    }

    pub(crate) fn ensure_session_for_open(
        &mut self,
    ) -> Option<(CommandPaneGroupId, CommandSessionId, bool)> {
        /*
        CDXC:CommandPane 2026-06-25-11:40:
        Opening an empty command pane mirrors macOS `openCommandsPanelForActiveProject`: create exactly one selected `Command Terminal` placeholder at open time. If a valid command tab already exists, preserve it and only expand/focus the pane so opening never invents extra tabs.
        */
        if let Some((group_id, session_id)) =
            self.active_group_and_session_id_in_dock(CommandPaneDock::Panel)
        {
            self.focused_group = group_id;
            self.expand();
            return Some((group_id, session_id, false));
        }

        // The Commands pane has no groups. Create its first one directly rather than through the
        // focused group, which may be a Terminal view group.
        let session_id = self.allocate_session_id();
        self.terminal_sessions
            .push(CommandTerminalSession::placeholder(
                session_id,
                COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
            ));
        let tab = CommandPaneTab { session_id };
        let group_id = self.replace_empty_command_layout_with_created_tab(
            CommandPaneDock::Panel,
            tab,
            session_id,
        );
        Some((group_id, session_id, true))
    }

    pub(crate) fn add_new_session(
        &mut self,
        target_group_id: Option<CommandPaneGroupId>,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:CommandPane 2026-06-25-21:21:
        Native command-panel New Terminal carries the clicked titlebar session as the insertion target, while keyboard creation uses the command panel's focused responder. Resolve the command group explicitly at the creation boundary so clicked plus/double-click chrome inserts into that group without depending on a prior focus side effect.
        */
        if let Some(group_id) = target_group_id
            && !self.focus_group(group_id)
        {
            return None;
        }

        let session_id = self.add_session_to_focused_group();
        Some((self.focused_group, session_id))
    }

    pub(crate) fn add_session_to_focused_group(&mut self) -> CommandSessionId {
        let session_id = self.allocate_session_id();
        self.add_titled_session_to_focused_group(
            session_id,
            COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
        );
        session_id
    }

    pub(crate) fn add_titled_session_to_focused_group(
        &mut self,
        session_id: CommandSessionId,
        title: String,
    ) {
        self.terminal_sessions
            .push(CommandTerminalSession::placeholder(session_id, title));
        let tab = CommandPaneTab { session_id };
        self.insert_created_tab_for_untargeted_creation(tab, session_id);
    }

    /// CDXC:CommandPane 2026-09-23 DECISION:
    /// User: the bottom command pane and the side panel's Terminal view are separate, so the collapsed bottom strip's + adds to the bottom pane: its focused group when that is docked at the bottom, else its first group. Untargeted, the strip used the last focused group, which could be the side panel's.
    pub(crate) fn bottom_dock_target_group(&self) -> Option<CommandPaneGroupId> {
        if command_node_contains_group(&self.root, self.focused_group) {
            Some(self.focused_group)
        } else {
            first_command_leaf_id(&self.root)
        }
    }

    pub(crate) fn live_group_for_untargeted_creation(&self) -> Option<CommandPaneGroupId> {
        /*
        CDXC:CommandPane 2026-06-26-04:29:
        Untargeted New Terminal creation must recover from a stale `focused_group` by using the first live command group, while explicit clicked-group creation still rejects missing targets before this path. This preserves the existing command split tree instead of replacing it with a new root leaf.

        CDXC:CommandPane 2026-06-27-04:36:
        Terminal Action creation no longer uses this focused-group fallback: newly-created non-reused Action tabs need native `createCommandTerminal(... focusAfterCreate:false)` placement, while Cmd+T/New Terminal keeps the focused command-group insertion rule.
        */
        self.find_leaf(self.focused_group)
            .filter(|leaf| !leaf.tab_group.tabs.is_empty())
            .map(|leaf| leaf.group_id)
            .or_else(|| first_command_leaf_id(&self.root))
    }

    pub(crate) fn insert_created_tab_for_untargeted_creation(
        &mut self,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        if let Some(group_id) = self.live_group_for_untargeted_creation() {
            return self.insert_created_tab_into_group(group_id, tab, session_id);
        }

        self.replace_empty_command_layout_with_created_tab(CommandPaneDock::Panel, tab, session_id)
    }

    pub(crate) fn insert_created_action_tab_for_untargeted_creation(
        &mut self,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        /*
        CDXC:CommandPane 2026-06-27-04:36:
        Newly-created non-reused terminal Actions follow native untargeted Action placement, not Cmd+T focus placement: an empty command layout creates the first owner, a single owner appends as a tab, and an existing split gets a new selected rightmost command owner without moving existing group memberships.
        */
        match command_node_leaf_count(&self.root) {
            0 => self.replace_empty_command_layout_with_created_tab(
                CommandPaneDock::Panel,
                tab,
                session_id,
            ),
            1 => {
                let group_id = first_command_leaf_id(&self.root)
                    .expect("single live command layout must have a command group");
                self.insert_created_tab_into_group(group_id, tab, session_id)
            }
            _ => self.insert_created_action_tab_as_rightmost_owner(tab, session_id),
        }
    }

    pub(crate) fn insert_created_tab_into_group(
        &mut self,
        group_id: CommandPaneGroupId,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        let leaf = self
            .find_leaf_mut(group_id)
            .expect("created command tab target group must exist");
        /*
        CDXC:CommandPane 2026-06-25-19:27:
        Native command-panel New Terminal uses `targetSessionId` only to find the command tab group; `addCommandSessionToPaneTabGroup` appends the new command session to the end of that group and then selects it. Keep `insert_session_at` as the exact-index API for command tab-strip transfer and reorder paths.

        CDXC:CommandPane 2026-06-27-04:36:
        Terminal Action creation shares this append path only when the command layout has a single live owner. Split layouts must create a separate Action owner so stale or unrelated command focus cannot pull the Action tab into an existing group.
        */
        leaf.tab_group
            .insert_session_at(tab, leaf.tab_group.tabs.len());
        leaf.tab_group.active_session = session_id;
        self.set_focused_group_for_selected_owner(group_id);
        self.clear_focus_mode_if_invalid();
        self.reveal_dock_for_group(group_id);
        group_id
    }

    pub(crate) fn replace_empty_command_layout_with_created_tab(
        &mut self,
        dock: CommandPaneDock,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        let group_id = self.allocate_group_id();
        *self.root_for_dock_mut(dock) = CommandPaneNode::Leaf(CommandPaneLeaf {
            group_id,
            tab_group: CommandPaneTabGroup {
                tabs: vec![tab],
                active_session: session_id,
            },
        });
        self.focused_group = group_id;
        self.focus_mode_group = None;
        self.reveal_dock_for_group(group_id);
        group_id
    }

    pub(crate) fn insert_created_action_tab_as_rightmost_owner(
        &mut self,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        /*
        CDXC:CommandPane 2026-06-27-04:36:
        Native `appendCommandSessionToPaneLayout` appends untargeted Action creation to an existing split as a separate rightmost command owner. GPUI represents that by wrapping the current command root as the first branch and the new Action leaf as the second branch, preserving all existing tab groups and their internal selections.
        */
        let existing_leaf_count = command_node_leaf_count(&self.root).max(1);
        let group_id = self.allocate_group_id();
        let split_id = self.allocate_split_id();
        let existing_root = std::mem::replace(&mut self.root, command_pane_dummy_node());
        self.root = CommandPaneNode::Split(CommandPaneSplit {
            id: split_id,
            axis: WorkspaceSplitAxis::Horizontal,
            ratio: workspace_split_ratio(
                existing_leaf_count as f32 / (existing_leaf_count + 1) as f32,
            ),
            first: Box::new(existing_root),
            second: Box::new(CommandPaneNode::Leaf(CommandPaneLeaf {
                group_id,
                tab_group: CommandPaneTabGroup {
                    tabs: vec![tab],
                    active_session: session_id,
                },
            })),
        });
        self.set_focused_group_for_selected_owner(group_id);
        self.clear_focus_mode_if_invalid();
        self.expand();
        group_id
    }

    pub(crate) fn split_session_adjacent_to_focused_group(
        &mut self,
        direction: FocusedTerminalSplitDirection,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:FocusMode 2026-06-25-16:05:
        Native command panels intentionally coerce both Cmd+D and Cmd+Shift+D to horizontal command splits. Keep GPUI command hotkey splits beside the focused command group while still storing split axis metadata for layout restore; do not create Agents tabs, processes, or terminal content.
        */
        let target_group_id = self
            .find_leaf(self.focused_group)
            .filter(|leaf| !leaf.tab_group.tabs.is_empty())
            .map(|leaf| leaf.group_id)?;
        let axis = command_pane_focused_split_axis(direction);
        let session_id = self.allocate_session_id();
        let group_id = self.allocate_group_id();
        let split_id = self.allocate_split_id();
        let new_leaf = CommandPaneLeaf {
            group_id,
            tab_group: CommandPaneTabGroup {
                tabs: vec![CommandPaneTab { session_id }],
                active_session: session_id,
            },
        };

        let Some(root) = self.root_for_group_mut(target_group_id) else {
            return None;
        };
        if insert_command_leaf_split(root, target_group_id, new_leaf, axis, false, split_id) {
            self.terminal_sessions
                .push(CommandTerminalSession::placeholder(
                    session_id,
                    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                ));
            self.focus_mode_group = None;
            self.focused_group = group_id;
            self.reveal_dock_for_group(group_id);
            Some((group_id, session_id))
        } else {
            None
        }
    }

    pub(crate) fn add_placeholder_session_from_workspace_title(
        &mut self,
        target_group_id: CommandPaneGroupId,
        title: String,
        zone: WorkspaceDropZone,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:CommandPane 2026-06-22-13:05:
        Workspace-to-command drops are command-pane placeholder creation, not command-tab movement. Allocate a command-only session id, keep the dragged Agents tab title for the live placeholder label, and map top/bottom intent to center grouping because command panes support only tab grouping and left/right horizontal splits.
        */
        let dock = self.dock_for_group(target_group_id)?;
        if command_pane_drop_zone_splits(dock, zone) {
            self.split_placeholder_session_from_workspace_title(target_group_id, title, zone)
        } else {
            self.group_placeholder_session_from_workspace_title(target_group_id, title)
        }
    }

    pub(crate) fn group_placeholder_session_from_workspace_title(
        &mut self,
        target_group_id: CommandPaneGroupId,
        title: String,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        let insertion_index = self.find_leaf(target_group_id)?.tab_group.tabs.len();
        self.insert_placeholder_session_from_workspace_title_at(
            target_group_id,
            insertion_index,
            title,
        )
    }

    pub(crate) fn insert_placeholder_session_from_workspace_title_at(
        &mut self,
        target_group_id: CommandPaneGroupId,
        insertion_index: usize,
        title: String,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:CommandPane 2026-06-22-16:18:
        Agents-to-command tab-strip drops are grouping operations at a command tab boundary, not command split operations. Insert a command-only placeholder with the visible Agents title at the requested index, select it, focus/expand the target group, and keep all real terminal/process/content state on the Agents side out of the command model.
        */
        self.find_leaf(target_group_id)?;
        let session_id = self.allocate_session_id();
        self.terminal_sessions
            .push(CommandTerminalSession::placeholder(session_id, title));
        let tab = CommandPaneTab { session_id };

        let Some(target_leaf) = self.find_leaf_mut(target_group_id) else {
            self.terminal_sessions
                .retain(|session| session.id != session_id);
            return None;
        };
        target_leaf
            .tab_group
            .insert_session_at(tab, insertion_index);
        target_leaf.tab_group.active_session = session_id;
        self.set_focused_group_for_selected_owner(target_group_id);
        self.clear_focus_mode_if_invalid();
        self.reveal_dock_for_group(target_group_id);
        Some((target_group_id, session_id))
    }

    pub(crate) fn split_placeholder_session_from_workspace_title(
        &mut self,
        target_group_id: CommandPaneGroupId,
        title: String,
        zone: WorkspaceDropZone,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        let dock = self.dock_for_group(target_group_id)?;
        if !command_pane_drop_zone_splits(dock, zone) {
            return self.group_placeholder_session_from_workspace_title(target_group_id, title);
        }

        let session_id = self.allocate_session_id();
        let group_id = self.allocate_group_id();
        let split_id = self.allocate_split_id();
        let new_leaf = CommandPaneLeaf {
            group_id,
            tab_group: CommandPaneTabGroup {
                tabs: vec![CommandPaneTab { session_id }],
                active_session: session_id,
            },
        };
        let axis = command_pane_drop_zone_split_axis(zone);
        let dragged_first = matches!(zone, WorkspaceDropZone::Left | WorkspaceDropZone::Top);

        let Some(root) = self.root_for_group_mut(target_group_id) else {
            return None;
        };
        if insert_command_leaf_split(
            root,
            target_group_id,
            new_leaf,
            axis,
            dragged_first,
            split_id,
        ) {
            self.terminal_sessions
                .push(CommandTerminalSession::placeholder(session_id, title));
            self.focus_mode_group = None;
            self.focused_group = group_id;
            self.reveal_dock_for_group(group_id);
            Some((group_id, session_id))
        } else {
            None
        }
    }
}
