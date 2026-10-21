use super::*;
use crate::*;

impl WorkspaceModel {
    pub(crate) fn session(&self, id: TerminalSessionId) -> Option<&TerminalSession> {
        self.terminal_sessions
            .iter()
            .find(|session| session.id == id)
    }

    pub(crate) fn has_session(&self, id: TerminalSessionId) -> bool {
        self.session(id).is_some()
    }

    pub(crate) fn terminal_session_ids(&self) -> Vec<TerminalSessionId> {
        self.terminal_sessions
            .iter()
            .map(|session| session.id)
            .collect()
    }

    pub(crate) fn session_is_mounting(&self, session_id: TerminalSessionId) -> bool {
        self.session(session_id).is_some_and(|session| {
            session.presentation_state == TerminalSessionPresentationState::Mounting
        })
    }

    #[allow(dead_code)] // no live caller: startup eligibility is decided by the surface-host mount path
    pub(crate) fn make_mounting_session_startup_eligible(
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
        if session.presentation_state != TerminalSessionPresentationState::Mounting {
            return false;
        }

        session.set_presentation_state_with_startup_eligibility(
            TerminalSessionPresentationState::Mounting,
            true,
        );
        true
    }

    pub(crate) fn transition_terminal_session_presentation_state(
        &mut self,
        session_id: TerminalSessionId,
        expected_state: TerminalSessionPresentationState,
        next_state: TerminalSessionPresentationState,
    ) -> bool {
        self.terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            .is_some_and(|session| {
                if session.presentation_state != expected_state {
                    return false;
                }

                session.set_presentation_state(next_state);
                true
            })
    }

    pub(crate) fn visible_selected_mounting_startup_candidates(
        &self,
    ) -> Vec<AgentsTerminalStartupRecord> {
        /*
        CDXC:Terminal 2026-06-22-23:50:
        Only rendered Agents leaves whose selected shell session is startup-eligible Mounting are startup candidates. Inactive tabs, hidden Focus-mode leaves, Running mount slots, failed placeholders, sleeping/restored/popped-out placeholders before activation, sleeping wake and popped-out reattach activations, restored presentation-only Mounting sessions, and missing-session tabs must not create startup records, duplicate popped-out runtimes, or real mount slots. Explicit restored-unmounted activation is the materialization exception and enters the existing startup pipeline.
        */
        self.rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                let leaf = self.find_leaf(pane_id)?;
                let shell_session_id = leaf.tab_group.active_session_id()?;
                self.session(shell_session_id)
                    .is_some_and(TerminalSession::can_enter_startup_pipeline)
                    .then_some(AgentsTerminalStartupRecord {
                        pane_id: leaf.pane_id,
                        shell_session_id,
                        startup_body_geometry_available: false,
                    })
            })
            .collect()
    }

    pub(crate) fn rendered_terminal_startup_body_slots(
        &self,
    ) -> Vec<AgentsTerminalStartupBodySlotId> {
        /*
        CDXC:Terminal 2026-06-23-00:10:
        Startup body slots identify only visible selected startup-eligible Mounting Agents terminal bodies for runtime launch preparation. They intentionally do not reuse `AgentsTerminalBodyMountSlotId`, because real Ghostty mount slots, Running host maps, and surface owners must remain restricted to visible selected Running sessions. Explicit restored-unmounted materialization may get a startup body slot; sleeping wake, popped-out reattach, and restored presentation-only Mounting after restart must not get hidden startup hosts.
        */
        self.rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                let leaf = self.find_leaf(pane_id)?;
                let session_id = leaf.tab_group.active_session_id()?;
                self.session(session_id)
                    .is_some_and(TerminalSession::can_enter_startup_pipeline)
                    .then_some(AgentsTerminalStartupBodySlotId {
                        pane_id: leaf.pane_id,
                        session_id,
                    })
            })
            .collect()
    }

    pub(crate) fn is_current_terminal_startup_body_slot(
        &self,
        slot_id: AgentsTerminalStartupBodySlotId,
    ) -> bool {
        self.rendered_terminal_startup_body_slots()
            .into_iter()
            .any(|current_slot_id| current_slot_id == slot_id)
    }

    pub(crate) fn terminal_body_mount_candidate(
        &self,
        leaf: &WorkspaceLeaf,
    ) -> AgentsTerminalBodyMountCandidate {
        let rendered_leaf_order = self.rendered_leaf_order();
        selected_agents_terminal_body_mount_candidate(
            leaf,
            &self.terminal_sessions,
            &rendered_leaf_order,
        )
    }

    pub(crate) fn rendered_terminal_body_mount_slots(&self) -> Vec<AgentsTerminalBodyMountSlotId> {
        /*
        CDXC:Terminal 2026-06-22-22:45:
        The pure all-visible mount-slot rule returns every rendered Agents leaf whose selected tab is Running. Focus mode hides leaves by narrowing rendered_leaf_order, inactive tabs never appear here, and the helper remains model-only so rendering cannot invent fallback surfaces or persisted runtime ids.
        */
        let rendered_leaf_order = self.rendered_leaf_order();
        rendered_leaf_order
            .into_iter()
            .filter_map(|pane_id| {
                let leaf = self.find_leaf(pane_id)?;
                let session_id = leaf.tab_group.active_session_id()?;
                self.session(session_id)
                    .is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Running
                    })
                    .then_some(AgentsTerminalBodyMountSlotId {
                        pane_id: leaf.pane_id,
                        session_id,
                    })
            })
            .collect()
    }

    pub(crate) fn is_current_terminal_body_mount_slot(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.rendered_terminal_body_mount_slots()
            .into_iter()
            .any(|current_slot_id| current_slot_id == slot_id)
    }
}
