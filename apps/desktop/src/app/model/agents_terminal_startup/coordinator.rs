use super::*;
use crate::*;

/*
CDXC:Terminal 2026-06-22-23:50:
Agents terminal startup is a runtime-only boundary keyed by process-local runtime session id, not by durable shell session id or body mount slot id. Visible selected Mounting tabs may become pending startup records, but this layer does not launch a process, infer success, persist runtime ids, log commands, store cwd/env/stdout/stderr, or create fallback surfaces.

CDXC:Terminal 2026-06-22-23:50:
Startup results are intentionally enum-only. Ready may promote the same current Mounting shell session to Running; Failed preserves the tab as a safe failed-startup placeholder with no raw error string, command text, path, environment, terminal content, or process details in shell state.

CDXC:Terminal 2026-06-23-00:10:
Visible selected Mounting Agents bodies need runtime-only startup geometry for future launch preparation, but the startup slot id stays separate from the Running-only libghostty body mount slot so geometry alone never creates Running host state, a Ghostty surface, process, or Running transition.

CDXC:Terminal 2026-06-23-00:22:
Phase 3 startup launch plans are a runtime-only readiness boundary for visible selected Mounting Agents bodies after exact body geometry exists. Plans may carry only runtime id, shell id, pane id, startup body slot id, bounds, and scale; they must not carry cwd, command, env, terminal content, stdout/stderr, process ids, logs, persisted fields, Ghostty hosts, Ghostty surfaces, Running mount slots, or Ready/Failed transitions by themselves.
*/
#[derive(Default)]
pub(crate) struct AgentsTerminalStartupCoordinator {
    pub(crate) pending_startups_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
    pub(crate) startup_launch_plans_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupLaunchPlan>,
    pub(crate) startup_completion_intents_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupCompletionIntent>,
}

impl AgentsTerminalStartupCoordinator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sync_visible_mounting_startup_candidates(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
        startup_body_geometries: &HashMap<
            AgentsTerminalStartupBodySlotId,
            AgentsTerminalStartupBodyGeometry,
        >,
    ) {
        let current_candidates = if agents_workspace_visible {
            workspace.visible_selected_mounting_startup_candidates()
        } else {
            Vec::new()
        };
        let current_startup_body_slot_ids = current_candidates
            .iter()
            .map(|candidate| candidate.startup_body_slot_id())
            .collect::<HashSet<_>>();

        self.pending_startups_by_runtime_session
            .retain(|runtime_session_id, record| {
                agents_workspace_visible
                    && current_startup_body_slot_ids.contains(&record.startup_body_slot_id())
                    && workspace
                        .session(record.shell_session_id)
                        .is_some_and(|session| {
                            session.presentation_state == TerminalSessionPresentationState::Mounting
                                && runtime_sessions
                                    .runtime_session_id_for_shell_session(record.shell_session_id)
                                    == Some(*runtime_session_id)
                        })
            });
        let pending_runtime_session_ids = self
            .pending_startups_by_runtime_session
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        self.startup_launch_plans_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });
        self.startup_completion_intents_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });

        if !agents_workspace_visible {
            return;
        }

        for mut candidate in current_candidates {
            let runtime_session_id =
                runtime_sessions.ensure_runtime_session_id(candidate.shell_session_id);
            candidate.startup_body_geometry_available =
                startup_body_geometries.contains_key(&candidate.startup_body_slot_id());
            self.pending_startups_by_runtime_session
                .insert(runtime_session_id, candidate);
        }
        self.sync_startup_completion_intents(agents_workspace_visible, workspace, runtime_sessions);
    }

    pub(crate) fn sync_startup_launch_plans(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        startup_body_geometries: &HashMap<
            AgentsTerminalStartupBodySlotId,
            AgentsTerminalStartupBodyGeometry,
        >,
    ) {
        self.startup_launch_plans_by_runtime_session = derive_agents_terminal_startup_launch_plans(
            agents_workspace_visible,
            workspace,
            runtime_sessions,
            startup_body_geometries,
            &self.pending_startups_by_runtime_session,
        );
        self.sync_startup_completion_intents(agents_workspace_visible, workspace, runtime_sessions);
    }

    pub(crate) fn sync_startup_completion_intents(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) {
        self.startup_completion_intents_by_runtime_session =
            derive_agents_terminal_startup_completion_intents(
                agents_workspace_visible,
                workspace,
                runtime_sessions,
                &self.pending_startups_by_runtime_session,
            );
    }

    #[allow(dead_code)]
    pub(crate) fn produce_startup_result_from_runtime_signal(
        &self,
        signal: Option<AgentsTerminalStartupCompletionSignal>,
    ) -> Option<AgentsTerminalStartupResult> {
        let signal = signal?;
        let completion_intent = signal.completion_intent();
        self.startup_completion_intents_by_runtime_session
            .get(&completion_intent.runtime_session_id)
            .copied()
            .is_some_and(|current_intent| current_intent == completion_intent)
            .then_some(signal.into_startup_result())
    }

    pub(crate) fn apply_startup_result(
        &mut self,
        workspace: &mut WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        result: AgentsTerminalStartupResult,
    ) -> bool {
        let runtime_session_id = result.runtime_session_id();
        let completion_intent = result.completion_intent();
        let Some(record) = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()
        else {
            return false;
        };

        if self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()
            != Some(completion_intent)
            || record.shell_session_id != completion_intent.shell_session_id
            || record.startup_body_slot_id() != completion_intent.startup_body_slot_id
        {
            return false;
        }

        if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
            != Some(runtime_session_id)
        {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            return false;
        }

        if !workspace.is_current_terminal_startup_body_slot(completion_intent.startup_body_slot_id)
        {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            return false;
        }

        let changed = workspace.transition_terminal_session_presentation_state(
            record.shell_session_id,
            TerminalSessionPresentationState::Mounting,
            result.terminal_presentation_state(),
        );
        if changed || !workspace.session_is_mounting(record.shell_session_id) {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
        }
        changed
    }

    pub(crate) fn startup_launch_plans(&self) -> Vec<AgentsTerminalStartupLaunchPlan> {
        let mut plans = self
            .startup_launch_plans_by_runtime_session
            .values()
            .copied()
            .collect::<Vec<_>>();
        plans.sort_by_key(|plan| {
            (
                plan.startup_body_slot_id.pane_id.0,
                plan.startup_body_slot_id.session_id.0,
                plan.runtime_session_id.0,
            )
        });
        plans
    }
}
