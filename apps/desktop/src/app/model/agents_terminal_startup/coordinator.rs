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

CDXC:Terminal 2026-06-23-03:23:
A render-start geometry reset must not churn an already-created hidden startup host/config while the same pending Mounting tab remains current. Preserve only hosts previously created from a launch plan and only by matching runtime id plus `AgentsTerminalStartupBodySlotId`; pending records without prior geometry must not create hosts.

CDXC:Terminal 2026-06-23-03:51:
GPUI has no exposed GhosttyKit tty/pid or terminalReady-equivalent signal yet, so startup completion is a runtime-only intent plus explicit signal boundary. A current Mounting tab may advertise an exact runtime/session/startup-slot intent, but the producer returns no Ready/Failed result without a real signal and must never infer success from hidden startup host or Ghostty surface creation.

CDXC:Terminal 2026-06-23-04:00:
Startup config preparation now has a runtime-only launch-payload source boundary, but GPUI does not currently populate it because no explicit app startup state carries cwd, command, env vars, initial input, or wait-after-command. The empty source keeps startup requests inert until a future explicit producer is wired, and invalid future payloads must prune the startup boundary instead of falling back to inferred values.

CDXC:Terminal 2026-06-23-04:13:
Ghostty startup surface metadata is now a real runtime-only readiness input, but it may only create a handoff plan for the exact current startup completion intent when Ghostty reports a tty name and foreground process while the process has not exited. Promotion may proceed only when startup host/surface ownership can be moved into the Running path without dropping or recreating the process, and this layer must not create Failed, persist ids, log metadata, or expose raw tty names/process ids.

CDXC:Terminal 2026-06-23-04:38:
Startup-owned Ghostty metadata is also the real runtime failure input. A process-exited snapshot may produce only the existing Failed result for the exact current runtime/session/startup-slot intent while the shell tab is still the visible selected Mounting body and the startup surface owner identity still matches; cleanup must drop the startup Ghostty surface before the hidden host and must not create Running maps, fallback success, logs, raw process details, paths, commands, env, or terminal content.
*/
#[derive(Default)]
pub(crate) struct AgentsTerminalStartupCoordinator {
    pub(crate) pending_startups_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
    pub(crate) startup_launch_plans_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupLaunchPlan>,
    pub(crate) startup_completion_intents_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupCompletionIntent>,
    pub(crate) startup_readiness_signal_preparations_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupReadinessSignalPreparation>,
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
        self.startup_readiness_signal_preparations_by_runtime_session
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
        self.startup_readiness_signal_preparations_by_runtime_session
            .retain(|runtime_session_id, preparation| {
                self.startup_completion_intents_by_runtime_session
                    .get(runtime_session_id)
                    .copied()
                    == Some(preparation.completion_intent)
            });
    }

    pub(crate) fn sync_startup_readiness_signal_preparations(
        &mut self,
        metadata_snapshots: impl IntoIterator<
            Item = (
                AgentsTerminalRuntimeSessionId,
                AgentsTerminalStartupBodySlotId,
                terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
            ),
        >,
    ) {
        self.startup_readiness_signal_preparations_by_runtime_session = metadata_snapshots
            .into_iter()
            .filter_map(
                |(runtime_session_id, startup_body_slot_id, surface_metadata)| {
                    self.prepare_startup_readiness_signal(
                        runtime_session_id,
                        startup_body_slot_id,
                        surface_metadata,
                    )
                    .map(|preparation| (runtime_session_id, preparation))
                },
            )
            .collect();
    }

    pub(crate) fn prepare_startup_readiness_signal(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<AgentsTerminalStartupReadinessSignalPreparation> {
        let completion_intent = self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;

        (completion_intent.runtime_session_id == runtime_session_id)
            .then_some(())
            .and_then(|_| {
                AgentsTerminalStartupReadinessSignalPreparation::new(
                    completion_intent,
                    startup_body_slot_id,
                    surface_metadata,
                )
            })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_readiness_handoff_plans(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) -> Vec<AgentsTerminalStartupReadinessHandoffPlan> {
        let mut plans = self
            .startup_readiness_signal_preparations_by_runtime_session
            .keys()
            .copied()
            .filter_map(|runtime_session_id| {
                self.startup_readiness_handoff_plan_for_runtime_session(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    runtime_session_id,
                )
            })
            .collect::<Vec<_>>();
        plans.sort_by_key(|plan| {
            (
                plan.startup_body_slot_id().pane_id.0,
                plan.startup_body_slot_id().session_id.0,
                plan.runtime_session_id().0,
            )
        });
        plans
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_readiness_handoff_plan_for_runtime_session(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Option<AgentsTerminalStartupReadinessHandoffPlan> {
        /*
        CDXC:Terminal 2026-06-23-04:25:
        A ready metadata snapshot may promote only the exact current Mounting body it was prepared for. Match runtime id, shell session id, startup body slot id, visible selected Mounting state, current launch plan, and the future Running mount slot before any owner map can move.
        */
        if !agents_workspace_visible {
            return None;
        }

        let preparation = self
            .startup_readiness_signal_preparations_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let completion_intent = preparation.completion_intent;
        if completion_intent.runtime_session_id != runtime_session_id
            || !preparation.surface_metadata.indicates_ready_metadata()
            || self
                .startup_completion_intents_by_runtime_session
                .get(&runtime_session_id)
                .copied()
                != Some(completion_intent)
        {
            return None;
        }

        let record = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let startup_launch_plan = self
            .startup_launch_plans_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let startup_body_slot_id = record.startup_body_slot_id();
        if record.shell_session_id != completion_intent.shell_session_id
            || startup_body_slot_id != completion_intent.startup_body_slot_id
            || startup_launch_plan.runtime_session_id != runtime_session_id
            || startup_launch_plan.shell_session_id != record.shell_session_id
            || startup_launch_plan.pane_id != record.pane_id
            || startup_launch_plan.startup_body_slot_id != startup_body_slot_id
            || runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            || !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id)
            || !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
        {
            return None;
        }

        let mount_slot_id = AgentsTerminalBodyMountSlotId {
            pane_id: startup_body_slot_id.pane_id,
            session_id: completion_intent.shell_session_id,
        };
        (mount_slot_id.session_id == startup_body_slot_id.session_id
            && mount_slot_id.pane_id == startup_launch_plan.pane_id)
            .then_some(AgentsTerminalStartupReadinessHandoffPlan {
                completion_intent,
                startup_launch_plan,
                mount_slot_id,
            })
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

    pub(crate) fn produce_failed_startup_result_from_surface_metadata(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<AgentsTerminalStartupResult> {
        if !agents_workspace_visible || !surface_metadata.process_exited() {
            return None;
        }

        let completion_intent = self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        if completion_intent.runtime_session_id != runtime_session_id
            || completion_intent.startup_body_slot_id != startup_body_slot_id
        {
            return None;
        }

        let record = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        if record.shell_session_id != completion_intent.shell_session_id
            || record.startup_body_slot_id() != startup_body_slot_id
            || runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            || !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id)
            || !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
        {
            return None;
        }

        Some(AgentsTerminalStartupResult::Failed { completion_intent })
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
            self.startup_readiness_signal_preparations_by_runtime_session
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
            self.startup_readiness_signal_preparations_by_runtime_session
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
            self.startup_readiness_signal_preparations_by_runtime_session
                .remove(&runtime_session_id);
        }
        changed
    }

    // Consumed by the macOS hidden-host startup path and by the non-macOS
    // GPUI-engine startup path, so it stays ungated.
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

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_host_preservation_keys(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) -> Vec<AgentsTerminalStartupHostPreservationKey> {
        let mut keys = derive_agents_terminal_startup_host_preservation_keys(
            agents_workspace_visible,
            workspace,
            runtime_sessions,
            &self.pending_startups_by_runtime_session,
        );
        keys.sort_by_key(|key| {
            (
                key.startup_body_slot_id.pane_id.0,
                key.startup_body_slot_id.session_id.0,
                key.runtime_session_id.0,
            )
        });
        keys
    }
}
