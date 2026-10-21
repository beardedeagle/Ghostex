use super::*;
use crate::*;

pub(crate) fn derive_agents_terminal_startup_launch_plans(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    startup_body_geometries: &HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupLaunchPlan> {
    if !agents_workspace_visible {
        return HashMap::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            let geometry = startup_body_geometries
                .get(&startup_body_slot_id)
                .copied()?;
            Some((
                runtime_session_id,
                AgentsTerminalStartupLaunchPlan {
                    runtime_session_id,
                    shell_session_id: record.shell_session_id,
                    pane_id: record.pane_id,
                    startup_body_slot_id,
                    bounds: geometry.bounds,
                    scale_factor: geometry.scale_factor,
                },
            ))
        })
        .collect()
}

pub(crate) fn derive_agents_terminal_startup_completion_intents(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupCompletionIntent> {
    if !agents_workspace_visible {
        return HashMap::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            Some((
                runtime_session_id,
                AgentsTerminalStartupCompletionIntent::from_record(runtime_session_id, record),
            ))
        })
        .collect()
}

pub(crate) fn prune_agents_terminal_startup_body_slot_geometries(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
) {
    let current_slot_ids = if agents_workspace_visible {
        agents_workspace.rendered_terminal_startup_body_slots()
    } else {
        Vec::new()
    };
    startup_body_geometries.retain(|slot_id, _| current_slot_ids.contains(slot_id));
}

pub(crate) fn record_agents_terminal_startup_body_slot_geometry(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    slot_id: AgentsTerminalStartupBodySlotId,
    bounds: Bounds<Pixels>,
    scale_factor: f32,
) {
    prune_agents_terminal_startup_body_slot_geometries(
        agents_workspace_visible,
        agents_workspace,
        startup_body_geometries,
    );

    if agents_workspace_visible && agents_workspace.is_current_terminal_startup_body_slot(slot_id) {
        startup_body_geometries.insert(
            slot_id,
            AgentsTerminalStartupBodyGeometry {
                bounds,
                scale_factor,
            },
        );
    } else {
        startup_body_geometries.remove(&slot_id);
    }
}

/// Failed or stale startup completion retires the startup-only state of that exact completion
/// intent: its body geometry and its explicit launch payload.
pub(crate) fn prune_agents_terminal_startup_runtime_state_for_completion_intent(
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_launch_payload_source: &mut AgentsTerminalStartupLaunchPayloadSource,
    completion_intent: AgentsTerminalStartupCompletionIntent,
) {
    startup_body_geometries.remove(&completion_intent.startup_body_slot_id);
    startup_launch_payload_source.remove_payload_for_completion_intent(completion_intent);
}
