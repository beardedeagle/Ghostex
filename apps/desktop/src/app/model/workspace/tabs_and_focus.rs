use super::*;
use crate::*;

impl WorkspaceModel {
    pub(crate) fn focus_pane(&mut self, pane_id: WorkspacePaneId) {
        if self.find_leaf_mut(pane_id).is_some() {
            self.set_focused_pane(pane_id);
            self.acknowledge_attention_for_active_session_in_pane(pane_id);
        }
    }

    /// CDXC:FocusRouting 2026-09-11 DECISION:
    /// User: with splits, typing goes to the last active pane among the panes still shown.
    /// Every focused-pane write records the pane here so a closed split or a pane that stops being rendered hands focus to the pane the user used last, not to the first leaf of the tree.
    pub(crate) fn set_focused_pane(&mut self, pane_id: WorkspacePaneId) {
        const PANE_FOCUS_HISTORY_LIMIT: usize = 32;
        self.focused_pane = pane_id;
        self.pane_focus_history.retain(|id| *id != pane_id);
        self.pane_focus_history.push(pane_id);
        if self.pane_focus_history.len() > PANE_FOCUS_HISTORY_LIMIT {
            let excess = self.pane_focus_history.len() - PANE_FOCUS_HISTORY_LIMIT;
            self.pane_focus_history.drain(..excess);
        }
    }

    /// The most recently focused pane that still exists and satisfies `accept`, newest first.
    pub(crate) fn most_recent_pane_where(
        &self,
        accept: impl Fn(&WorkspaceLeaf) -> bool,
    ) -> Option<WorkspacePaneId> {
        self.pane_focus_history
            .iter()
            .rev()
            .copied()
            .find(|pane_id| self.find_leaf(*pane_id).is_some_and(&accept))
    }

    pub(crate) fn prune_pane_focus_history(&mut self) {
        let leaves = self.leaf_order();
        self.pane_focus_history
            .retain(|pane_id| leaves.contains(pane_id));
    }

    pub(crate) fn select_tab(&mut self, pane_id: WorkspacePaneId, session_id: TerminalSessionId) {
        let tab_selected = self.find_leaf_mut(pane_id).is_some_and(|leaf| {
            if leaf.tab_group.has_session(session_id) {
                leaf.tab_group.active_tab = session_id;
                true
            } else {
                false
            }
        });

        if tab_selected {
            self.set_focused_pane(pane_id);
            self.acknowledge_attention_for_session_activation(session_id);
        }
    }

    pub(crate) fn active_session_in_pane(
        &self,
        pane_id: WorkspacePaneId,
    ) -> Option<TerminalSessionId> {
        self.find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_session_id())
    }

    pub(crate) fn acknowledge_attention_for_session_activation(
        &mut self,
        session_id: TerminalSessionId,
    ) -> bool {
        let Some(session) = self
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        if session.activity != AgentTerminalActivity::Attention {
            return false;
        }
        session.activity = AgentTerminalActivity::Idle;
        true
    }

    pub(crate) fn acknowledge_attention_for_active_session_in_pane(
        &mut self,
        pane_id: WorkspacePaneId,
    ) -> bool {
        let Some(session_id) = self.active_session_in_pane(pane_id) else {
            return false;
        };
        self.acknowledge_attention_for_session_activation(session_id)
    }

    pub(crate) fn session_belongs_to_pane(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        self.session(session_id).is_some()
            && self
                .find_leaf(pane_id)
                .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
    }

    pub(crate) fn pane_id_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<WorkspacePaneId> {
        self.leaf_order().into_iter().find(|pane_id| {
            self.find_leaf(*pane_id)
                .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
        })
    }

    pub(crate) fn activate_terminal_placeholder_session(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:SessionSleep 2026-06-22-23:33:
        Agents terminal tab selection must not auto-wake sleeping/restored/popped-out sessions or auto-retry failed startup sessions, but activating the selected placeholder body or card button should move those presentations into Mounting so wake, materialize, reattach, and retry stay honest pending runtime-startup states. Existing Running sessions remain Running, Mounting remains pending, and this slice must not launch a process, synthesize terminal success, persist runtime data, or create terminal content.

        CDXC:Terminal 2026-06-23-18:00:
        Sleep and popped-out activation are not new-terminal startup and must stay blocked from hidden startup host/surface creation. Slice 236 handles them only through exact parked-owner transfer, while failed startup retry and explicit restored-unmounted materialization are the placeholder activations that may reuse the startup pipeline.

        CDXC:Terminal 2026-06-23-18:19:
        The durable workspace model marks explicit failed-startup retry and restored-unmounted materialization as startup-eligible Mounting. Process-local retry attempt id rotation belongs to the app/runtime helper and remains limited to failed-startup retry so shell state never owns or persists runtime ids.

        CDXC:Terminal 2026-06-23-19:26:
        Explicit restored-unmounted activation materializes through startup-eligible Mounting using the durable shell session's existing process-local runtime id. It must not rotate a retry runtime id, and tab selection alone remains presentation-only.
        */
        if self.session(session_id).is_none() {
            return false;
        }

        let selection_changed = {
            let Some(leaf) = self.find_leaf_mut(pane_id) else {
                return false;
            };
            if !leaf.tab_group.has_session(session_id) {
                return false;
            }

            let selection_changed = leaf.tab_group.active_tab != session_id;
            leaf.tab_group.active_tab = session_id;
            selection_changed
        };

        let focus_changed = self.focused_pane != pane_id;
        self.set_focused_pane(pane_id);

        let presentation_changed = self
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            .is_some_and(|session| {
                if let Some(next_state) = session.presentation_state.activation_pending_state() {
                    let startup_eligible = matches!(
                        session.presentation_state,
                        TerminalSessionPresentationState::StartupFailed
                            | TerminalSessionPresentationState::RestoredUnmounted
                    );
                    session.set_presentation_state_with_startup_eligibility(
                        next_state,
                        startup_eligible,
                    );
                    true
                } else {
                    false
                }
            });

        selection_changed || focus_changed || presentation_changed
    }

    pub(crate) fn cycle_tab_in_pane(&mut self, pane_id: WorkspacePaneId, reverse: bool) -> bool {
        /*
        CDXC:FocusRouting 2026-06-22-06:02:
        Shell tab cycling must operate inside the focused Agents tab group and include sleeping, mounting, failed-startup, restored/unmounted, and popped-out placeholders as ordinary tabs. Cycling changes only the active tab id; it must not wake, mount, materialize, retry, or reattach placeholder sessions.
        */
        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            return false;
        };
        leaf.tab_group.cycle_active_session(reverse).is_some()
    }

    #[allow(dead_code)] // no caller: tab closing goes through the id-addressed close paths
    pub(crate) fn close_active_tab(&mut self) -> bool {
        /*
        CDXC:FocusRouting 2026-06-22-06:02:
        Closing an Agents placeholder tab is shell state only in this slice. Remove the active tab from the focused tab group, select a neighbor when possible, and collapse an emptied split branch.

        CDXC:Workarea 2026-06-26-05:23:
        GPUI Agents close must match the macOS workspace: closing the last visible terminal is a real close that leaves an empty workspace pane instead of preserving a fake sleeping or final-root terminal.
        */
        let pane_id = self.focused_pane;
        let Some(session_id) = self
            .find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_session_id())
        else {
            return false;
        };
        self.close_tab(pane_id, session_id)
    }

    pub(crate) fn close_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        if !self.session_belongs_to_pane(pane_id, session_id) {
            return false;
        }
        let Some((_tab, source_is_empty)) = self.remove_tab_for_move(pane_id, session_id) else {
            return false;
        };
        self.terminal_sessions
            .retain(|session| session.id != session_id);

        if self.terminal_sessions.is_empty() {
            self.root = workspace_empty_leaf_node(pane_id);
            self.set_focused_pane(pane_id);
            self.focus_mode_pane = None;
            return true;
        }
        if source_is_empty {
            self.collapse_empty_leaf(pane_id);
        }
        self.normalize_workspace_tree();
        true
    }

    pub(crate) fn close_tab_from_direct_tab_close(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-26-06:57:
        Direct Agents tab close mirrors native pane-tab close: select the clicked tab before removal so inactive tab close elects the right sibling, then left sibling, from the close target instead of from a previously active tab. Scoped Close Right/Left/Others keep their no-focus native menu semantics and call `close_tab` directly.
        */
        if !self.session_belongs_to_pane(pane_id, session_id) {
            return false;
        }
        self.select_tab(pane_id, session_id);
        self.close_tab(pane_id, session_id)
    }

    pub(crate) fn selected_session_after_direct_tab_close(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> Option<TerminalSessionId> {
        /*
        CDXC:Workarea 2026-06-26-07:25:
        GPUI must tell the sidebar runtime which pane-local session should become focused after a direct native tab Close. Simulate the existing direct-close reducer on a clone so the asynchronous sidebar cleanup receives the same right-then-left or surviving-pane target that the local workspace applies immediately.
        */
        let mut next = self.clone();
        if !next.close_tab_from_direct_tab_close(pane_id, session_id) {
            return None;
        }
        next.find_leaf(next.focused_pane)
            .and_then(|leaf| leaf.tab_group.active_session_id())
    }

    pub(crate) fn tab_session_ids_for_close_scope(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabCloseScope,
    ) -> Vec<TerminalSessionId> {
        /*
        CDXC:ContextMenus 2026-06-26-06:57:
        Agents tab context-menu close scopes resolve only inside the clicked workspace pane tab group, matching macOS paneLayout behavior. The resolver uses ids only and never crosses split panes, command tabs, Browser tabs, project-editor surfaces, titles, paths, command text, terminal output, or persisted gxserver metadata.
        */
        let Some(leaf) = self.find_leaf(pane_id) else {
            return Vec::new();
        };
        let tab_session_ids = leaf
            .tab_group
            .tabs
            .iter()
            .map(|tab| tab.session_id)
            .collect::<Vec<_>>();
        let Some(tab_index) = tab_session_ids
            .iter()
            .position(|candidate| *candidate == session_id)
        else {
            return Vec::new();
        };

        match scope {
            AgentsWorkspaceTabCloseScope::Close => vec![session_id],
            AgentsWorkspaceTabCloseScope::CloseLeft => tab_session_ids[..tab_index].to_vec(),
            AgentsWorkspaceTabCloseScope::CloseOthers => tab_session_ids
                .into_iter()
                .filter(|candidate| *candidate != session_id)
                .collect(),
            AgentsWorkspaceTabCloseScope::CloseRight => tab_session_ids[tab_index + 1..].to_vec(),
        }
    }

    pub(crate) fn tab_session_ids_for_sleep_scope(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabSleepScope,
    ) -> Vec<TerminalSessionId> {
        /*
        CDXC:SessionSleep 2026-06-26-06:57:
        Agents tab Sleep scopes use the same pane-local sibling list as native pane tabs. Sleeping tabs remain in the layout, so the resolver returns ids only and leaves lifecycle mutation, mounted-owner parking, and focus replacement to the explicit sleep path.
        */
        let Some(leaf) = self.find_leaf(pane_id) else {
            return Vec::new();
        };
        let tab_session_ids = leaf
            .tab_group
            .tabs
            .iter()
            .map(|tab| tab.session_id)
            .collect::<Vec<_>>();
        let Some(tab_index) = tab_session_ids
            .iter()
            .position(|candidate| *candidate == session_id)
        else {
            return Vec::new();
        };

        match scope {
            AgentsWorkspaceTabSleepScope::Sleep => vec![session_id],
            AgentsWorkspaceTabSleepScope::SleepLeft => tab_session_ids[..tab_index].to_vec(),
            AgentsWorkspaceTabSleepScope::SleepOthers => tab_session_ids
                .into_iter()
                .filter(|candidate| *candidate != session_id)
                .collect(),
            AgentsWorkspaceTabSleepScope::SleepRight => tab_session_ids[tab_index + 1..].to_vec(),
        }
    }

    pub(crate) fn set_session_sleeping(
        &mut self,
        session_id: TerminalSessionId,
        is_sleeping: bool,
    ) -> bool {
        let Some(session) = self
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        let next_state = if is_sleeping {
            TerminalSessionPresentationState::Sleeping
        } else {
            TerminalSessionPresentationState::Mounting
        };
        if session.presentation_state == next_state {
            return false;
        }
        if !is_sleeping && session.presentation_state != TerminalSessionPresentationState::Sleeping
        {
            return false;
        }

        /*
        CDXC:SessionSleep 2026-06-26-06:57:
        Agents Sleep is a shell lifecycle mutation that parks the current terminal owner through the existing Sleeping presentation state. Preserve delayed-send intent but clear visible work/attention activity, and never create startup launch payloads, fallback Running state, command text, paths, terminal output, logs, or persistent gxserver transition data from this model helper.
        */
        session.set_presentation_state_with_startup_eligibility(next_state, false);
        if is_sleeping {
            session.activity = AgentTerminalActivity::Idle;
        }
        true
    }

    pub(crate) fn select_replacement_after_direct_tab_sleep(
        &mut self,
        pane_id: WorkspacePaneId,
        slept_session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:SessionSleep 2026-06-26-06:57:
        Direct native pane-tab Sleep uses the clicked tab group as transition origin: if the active clicked tab goes sleeping, choose the next awake right sibling, then left sibling, and leave all-sleeping groups selected on the sleeping placeholder. Sibling scoped Sleep rows intentionally do not retarget focus.
        */
        let Some(replacement_session_id) =
            self.replacement_session_after_direct_tab_sleep(pane_id, slept_session_id)
        else {
            return false;
        };
        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            return false;
        };
        if leaf.tab_group.active_tab == replacement_session_id {
            return false;
        }
        leaf.tab_group.active_tab = replacement_session_id;
        true
    }

    pub(crate) fn replacement_session_after_direct_tab_sleep(
        &self,
        pane_id: WorkspacePaneId,
        slept_session_id: TerminalSessionId,
    ) -> Option<TerminalSessionId> {
        /*
        CDXC:Workarea 2026-06-26-07:25:
        Direct native tab Sleep uses pane-tab transition origin only when the slept tab is currently active, then selects the next awake right sibling before left siblings. Keep this pure helper shared with the sidebar lifecycle bridge so Rust reports the same replacement target it will later apply locally.
        */
        let leaf = self.find_leaf(pane_id)?;
        if leaf.tab_group.active_session_id() != Some(slept_session_id) {
            return None;
        }
        let tab_session_ids = leaf
            .tab_group
            .tabs
            .iter()
            .map(|tab| tab.session_id)
            .collect::<Vec<_>>();
        let slept_index = tab_session_ids
            .iter()
            .position(|candidate| *candidate == slept_session_id)?;
        tab_session_ids[slept_index + 1..]
            .iter()
            .chain(tab_session_ids[..slept_index].iter().rev())
            .copied()
            .find(|candidate| {
                self.session(*candidate).is_some_and(|session| {
                    session.presentation_state != TerminalSessionPresentationState::Sleeping
                })
            })
    }

    pub(crate) fn can_close_tab(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        self.session_belongs_to_pane(pane_id, session_id)
    }

    pub(crate) fn can_transfer_tab_to_command_pane(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-26-05:23:
        Closing the final Agents tab is allowed for real close parity, but Agents-to-command transfers still require a surviving Agents tab or pane so the transaction cannot move the whole workspace into the command panel.
        */
        let Some(leaf) = self.find_leaf(pane_id) else {
            return false;
        };
        leaf.tab_group.has_session(session_id)
            && (leaf.tab_group.tabs.len() > 1 || self.leaf_order().len() > 1)
    }
}
