// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: Agents terminal sync: runtime ids, startup candidates and the composited engine terminals.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn sync_agents_terminal_surface_host(&mut self, cx: &mut gpui::Context<Self>) {
        // A window closing beside another one detaches its terminals; that is not their sessions
        // exiting (app/workspace_windows/).
        if self.workspace_window_closing {
            return;
        }
        /*
        CDXC:SessionIdentity 2026-06-22-23:24:
        Runtime session ids are reconciled from shell `TerminalSessionId` before terminal sync. The shell id remains the persisted layout identity across moves/splits/restore, while the runtime id is private app-process state that prunes when shell sessions disappear and must never be serialized or used as a title.

        CDXC:KeepAwake 2026-06-26-00:29:
        Startup and engine sync can change whether an existing safe `AgentTerminalActivity::Working` row counts as running. Compare the working-session count across this lifecycle boundary and resync Keep Awake automation only from app-owned model state.
        */
        let working_session_count_before =
            gpui_keep_awake_working_session_count(&self.agents_workspace, &self.command_pane);
        self.agents_terminal_runtime_sessions
            .reconcile_with_workspace(&self.agents_workspace);
        self.sync_agents_gpui_engine_terminals(cx);
        /*
        Reconcile any Chat launch intent that could not be consumed when its
        shell mapping was first created (for example, until agent metadata
        arrived). The terminal runtime remains live behind the Chat surface.
        */
        self.reconcile_preferred_agents_chat_launch_intents(cx);
        prune_agents_terminal_startup_body_slot_geometries(
            self.agents_workspace_visible(),
            &self.agents_workspace,
            &mut self.agents_terminal_startup_body_slot_geometries,
        );
        self.agents_terminal_startup_coordinator
            .sync_visible_mounting_startup_candidates(
                self.agents_workspace_visible(),
                &self.agents_workspace,
                &mut self.agents_terminal_runtime_sessions,
                &self.agents_terminal_startup_body_slot_geometries,
            );
        self.agents_terminal_startup_coordinator
            .sync_startup_launch_plans(
                self.agents_workspace_visible(),
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &self.agents_terminal_startup_body_slot_geometries,
            );
        self.spawn_agents_terminal_startup_gpui_engine_terminals(cx);
        let working_session_count_after =
            gpui_keep_awake_working_session_count(&self.agents_workspace, &self.command_pane);
        if working_session_count_before != working_session_count_after {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        self.agents_terminal_launch_payload_source
            .retain_live_mount_slots(
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
            );
    }

    /// Applies a startup result from the GPUI-engine spawn path to the startup coordinator and
    /// retires the startup-only state of a completed or stale-pruned intent.
    pub(crate) fn apply_agents_terminal_startup_result(
        &mut self,
        result: AgentsTerminalStartupResult,
    ) -> bool {
        let completion_intent = result.completion_intent();
        let had_current_completion_intent = self
            .agents_terminal_startup_coordinator
            .startup_completion_intents_by_runtime_session
            .get(&completion_intent.runtime_session_id)
            .copied()
            == Some(completion_intent);
        let changed = self
            .agents_terminal_startup_coordinator
            .apply_startup_result(
                &mut self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                result,
            );
        let exact_completion_intent_pruned = had_current_completion_intent
            && !self
                .agents_terminal_startup_coordinator
                .startup_completion_intents_by_runtime_session
                .contains_key(&completion_intent.runtime_session_id);
        if changed || exact_completion_intent_pruned {
            prune_agents_terminal_startup_runtime_state_for_completion_intent(
                &mut self.agents_terminal_startup_body_slot_geometries,
                &mut self.agents_terminal_startup_launch_payload_source,
                completion_intent,
            );
        }
        changed
    }
}
