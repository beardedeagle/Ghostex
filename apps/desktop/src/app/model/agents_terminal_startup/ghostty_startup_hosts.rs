use super::*;
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminal_ghostty_surface_config_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> terminal_ghostty_surface::GhosttySurfaceTerminalConfig {
    /*
    CDXC:Terminal 2026-06-24-11:27:
    GPUI embedded terminal surfaces consume the shared Settings service directly for supported `ghostty_surface_config_s` fields. Only `terminalFontSize` maps to the current FFI request as `font_size`; other Ghostty settings remain unthreaded here because GPUI has no safe direct runtime field or reload contract for them yet.

    CDXC:Terminal 2026-06-27-10:10:
    Command-pane Ghostty surfaces share this bounded GPUI terminal-settings mapper with Agents surfaces. Apply the FFI-supported `terminalFontSize` to recreated/prepared surface requests, and keep font family, theme, cursor, scrollback, clipboard, paste-preview, and mouse settings on the Ghostty config-file path until GhosttyKit exposes a safe live request field or reload contract.
    */
    let terminal_config = settings.terminal_ghostty_surface_config();
    terminal_ghostty_surface::GhosttySurfaceTerminalConfig::with_font_size(
        terminal_config.font_size(),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn current_gpui_terminal_ghostty_surface_config()
-> terminal_ghostty_surface::GhosttySurfaceTerminalConfig {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    gpui_terminal_ghostty_surface_config_from_shared_settings(&settings)
}

#[cfg(target_os = "macos")]
pub(crate) fn reconcile_agents_terminal_startup_host_config_requests<F>(
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_surface_owners: Option<
        &mut HashMap<
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::StartupGhosttySurfaceOwner,
        >,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    startup_launch_payload_source: &AgentsTerminalStartupLaunchPayloadSource,
    terminal_config: terminal_ghostty_surface::GhosttySurfaceTerminalConfig,
    parent_ns_view: *mut std::ffi::c_void,
    create_host_view: F,
) where
    F: FnMut(
        terminal_native_view::TerminalHostNativeViewCreateRequest,
    ) -> Result<
        terminal_native_view::OwnedTerminalHostNativeView,
        terminal_native_view::TerminalHostNativeViewCreateError,
    >,
{
    /*
    CDXC:Terminal 2026-06-23-03:23:
    Startup host/config request reconciliation creates hidden host views only from current Mounting launch plans with exact geometry. If render-start clears geometry before the next body canvas records, preserve an already-owned startup host/config only when the current pending record still matches the same runtime id and `AgentsTerminalStartupBodySlotId`; stale pending state, invalid parent/bounds/config, or missing current records must drop the runtime-only state.

    CDXC:Terminal 2026-06-23-04:00:
    Startup config requests may receive a launch payload only from the runtime-only explicit source for the same launch plan identity. If a future explicit payload fails validation, skip the config request so the hidden startup host/surface is pruned without falling back to terminal titles, status text, project paths, sidebar labels, delayed-send state, or inferred cwd/command/env values.
    */
    terminal_native_view::reconcile_app_owned_terminal_startup_host_native_view(
        startup_host_native_views,
        startup_launch_plans,
        startup_host_preservation_keys,
        parent_ns_view,
        create_host_view,
    );

    let current_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    *startup_config_requests = startup_host_native_views
        .iter()
        .filter_map(|(slot_id, host_view)| {
            let plan = current_launch_plans_by_slot
                .get(slot_id)
                .copied()
                .unwrap_or_else(|| host_view.startup_launch_plan());
            let request =
                terminal_native_view::ghostty_surface_config_request_for_app_owned_terminal_startup_host_native_view(
                    Some(host_view),
                )
                .ok()
                .flatten()?;
            let request = request.with_terminal_config(terminal_config);
            let launch_payload = startup_launch_payload_source
                .payload_for_launch_plan(plan)
                .ok()?;
            let request = if let Some(launch_payload) = launch_payload {
                request.with_launch_payload(launch_payload)
            } else {
                request
            };
            Some((*slot_id, request))
        })
        .collect();
    if let Some(startup_surface_owners) = startup_surface_owners {
        let startup_host_slots_without_config = startup_host_native_views
            .keys()
            .copied()
            .filter(|slot_id| !startup_config_requests.contains_key(slot_id))
            .collect::<Vec<_>>();
        for slot_id in startup_host_slots_without_config {
            startup_surface_owners.remove(&slot_id);
        }
    }
    startup_host_native_views.retain(|slot_id, _| startup_config_requests.contains_key(slot_id));
}

#[cfg(target_os = "macos")]
pub(crate) fn drop_agents_terminal_startup_ghostty_surface_owners_before_host_reconcile(
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_host_native_views: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    parent_ns_view: *mut std::ffi::c_void,
) {
    let startup_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    let startup_host_preservation_keys_by_slot = startup_host_preservation_keys
        .iter()
        .copied()
        .map(|key| (key.startup_body_slot_id, key))
        .collect::<HashMap<_, _>>();
    let stale_surface_slot_ids = startup_surface_owners
        .keys()
        .copied()
        .filter(|slot_id| {
            let Some(host_view) = startup_host_native_views.get(slot_id) else {
                return true;
            };

            !terminal_native_view::app_owned_terminal_startup_host_native_view_will_survive_reconcile(
                host_view,
                startup_launch_plans_by_slot.get(slot_id).copied(),
                startup_host_preservation_keys_by_slot.get(slot_id).copied(),
                parent_ns_view,
            )
        })
        .collect::<Vec<_>>();

    for slot_id in stale_surface_slot_ids {
        startup_surface_owners.remove(&slot_id);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn reconcile_agents_terminal_startup_ghostty_surface_owners<F>(
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    ghostty_app: &mut Option<terminal_ghostty_surface::GhosttyAppOwner>,
    startup_config_requests: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    mut create_ghostty_app: F,
) where
    F: FnMut() -> Result<
        terminal_ghostty_surface::GhosttyAppOwner,
        terminal_ghostty_surface::GhosttySurfaceRuntimeError,
    >,
{
    /*
    CDXC:Terminal 2026-06-23-03:33:
    Startup Ghostty surface owners are runtime-only consumers of prepared startup config requests and launch-created geometry. Create only when a matching config request and launch plan exist, preserve same-slot/same-runtime owners across geometry-gap preservation, and drop stale or invalid owners without showing/focusing hosts, applying startup results, logging, persisting, or touching Running mount-slot maps.
    */
    let startup_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    let startup_host_preservation_keys_by_slot = startup_host_preservation_keys
        .iter()
        .copied()
        .map(|key| (key.startup_body_slot_id, key))
        .collect::<HashMap<_, _>>();

    startup_surface_owners.retain(|slot_id, owner| {
        if !startup_config_requests.contains_key(slot_id)
            || owner.startup_body_slot_id() != *slot_id
        {
            return false;
        }

        if let Some(plan) = startup_launch_plans_by_slot.get(slot_id) {
            owner.runtime_session_id() == plan.runtime_session_id
        } else {
            startup_host_preservation_keys_by_slot
                .get(slot_id)
                .is_some_and(|key| owner.runtime_session_id() == key.runtime_session_id)
        }
    });

    for plan in startup_launch_plans {
        let plan = *plan;
        let slot_id = plan.startup_body_slot_id;
        if !startup_launch_plans_by_slot
            .get(&slot_id)
            .is_some_and(|current_plan| *current_plan == plan)
        {
            continue;
        }

        let Some(request) = startup_config_requests.get(&slot_id) else {
            startup_surface_owners.remove(&slot_id);
            continue;
        };
        if terminal_ghostty_surface::GhosttySurfacePixelSize::from_gpui_bounds(
            plan.bounds,
            f64::from(plan.scale_factor),
        )
        .is_err()
        {
            startup_surface_owners.remove(&slot_id);
            continue;
        }

        if startup_surface_owners.get(&slot_id).is_some_and(|owner| {
            owner.startup_body_slot_id() != slot_id
                || owner.runtime_session_id() != plan.runtime_session_id
        }) {
            startup_surface_owners.remove(&slot_id);
        }

        if !startup_surface_owners.contains_key(&slot_id) {
            if ghostty_app.is_none() {
                let Ok(app) = create_ghostty_app() else {
                    startup_surface_owners.clear();
                    return;
                };
                *ghostty_app = Some(app);
            }

            let Some(app) = ghostty_app.as_ref() else {
                return;
            };
            let Ok(surface) = terminal_ghostty_surface::StartupGhosttySurfaceOwner::new(
                app,
                slot_id,
                plan.runtime_session_id,
                request,
            ) else {
                startup_surface_owners.remove(&slot_id);
                continue;
            };
            startup_surface_owners.insert(slot_id, surface);
        }

        let update_failed = startup_surface_owners
            .get_mut(&slot_id)
            .is_some_and(|surface| {
                surface
                    .update_content_scale_and_size(plan.bounds, f64::from(plan.scale_factor))
                    .is_err()
            });
        if update_failed {
            startup_surface_owners.remove(&slot_id);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_startup_surface_metadata_snapshots(
    startup_surface_owners: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
) -> Vec<(
    AgentsTerminalRuntimeSessionId,
    AgentsTerminalStartupBodySlotId,
    terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
)> {
    /*
    CDXC:Terminal 2026-06-23-04:13:
    Reading startup surface metadata prepares only a runtime handoff fact for an exact current startup intent. The caller may promote Ready only through the startup-to-Running owner transfer path, so metadata alone still cannot fake Running, create Failed, persist ids, log tty/process facts, or expose raw terminal data.

    CDXC:Terminal 2026-06-23-04:38:
    Surface metadata is sampled only from the current startup-owned surface map entry whose key matches the owner's startup body slot. The snapshot carries redacted booleans only, so runtime failure handling can distinguish process-exited from ready metadata without exposing raw pid, tty, command, cwd/path, env, output, terminal content, or runtime ids outside runtime memory.
    */
    startup_surface_owners
        .iter()
        .filter(|&(startup_body_slot_id, surface)| {
            surface.startup_body_slot_id() == *startup_body_slot_id
        })
        .map(|(startup_body_slot_id, surface)| {
            (
                surface.runtime_session_id(),
                *startup_body_slot_id,
                surface.metadata_snapshot(),
            )
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn failed_agents_terminal_startup_results_from_metadata(
    startup_coordinator: &AgentsTerminalStartupCoordinator,
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    metadata_snapshots: impl IntoIterator<
        Item = (
            AgentsTerminalRuntimeSessionId,
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
        ),
    >,
) -> Vec<AgentsTerminalStartupResult> {
    metadata_snapshots
        .into_iter()
        .filter_map(
            |(runtime_session_id, startup_body_slot_id, surface_metadata)| {
                startup_coordinator.produce_failed_startup_result_from_surface_metadata(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    runtime_session_id,
                    startup_body_slot_id,
                    surface_metadata,
                )
            },
        )
        .collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn sync_agents_terminal_startup_readiness_signal_preparations(
    startup_coordinator: &mut AgentsTerminalStartupCoordinator,
    metadata_snapshots: impl IntoIterator<
        Item = (
            AgentsTerminalRuntimeSessionId,
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
        ),
    >,
) {
    startup_coordinator.sync_startup_readiness_signal_preparations(metadata_snapshots);
}

#[cfg(target_os = "macos")]
pub(crate) fn prune_agents_terminal_startup_runtime_state_for_completion_intent(
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_launch_payload_source: &mut AgentsTerminalStartupLaunchPayloadSource,
    completion_intent: AgentsTerminalStartupCompletionIntent,
) {
    /*
    CDXC:Terminal 2026-06-23-04:38:
    Failed startup cleanup retires startup-only runtime state for the exact completion intent. Remove the startup Ghostty surface before its hidden AppKit host, remove prepared config/geometry/payload state, and leave the shell session as the retryable StartupFailed placeholder without creating Running ownership.
    */
    startup_body_geometries.remove(&completion_intent.startup_body_slot_id);
    startup_surface_owners.remove(&completion_intent.startup_body_slot_id);
    startup_config_requests.remove(&completion_intent.startup_body_slot_id);
    startup_host_native_views.remove(&completion_intent.startup_body_slot_id);
    startup_launch_payload_source.remove_payload_for_completion_intent(completion_intent);
}

#[cfg(not(target_os = "macos"))]
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

#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_attachment_plan_for_startup_handoff(
    handoff_plan: AgentsTerminalStartupReadinessHandoffPlan,
) -> terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
    terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
        host_id: terminal_surface_host::NativeTerminalSurfaceHostId::from_slot_id(
            handoff_plan.mount_slot_id,
        ),
        slot_id: handoff_plan.mount_slot_id,
        bounds: handoff_plan.startup_launch_plan.bounds,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn transfer_ready_agents_terminal_startup_handoff(
    startup_coordinator: &mut AgentsTerminalStartupCoordinator,
    workspace: &mut WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    running_mount_slot_bounds: &mut HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    running_config_requests: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    handoff_plan: AgentsTerminalStartupReadinessHandoffPlan,
) -> bool {
    /*
    CDXC:Terminal 2026-06-23-04:25:
    Ready startup promotion is a single ownership move, not a new launch. Require exact current startup readiness plus empty target Running owner maps, remove the startup host/surface only into local ownership, promote the same shell session to Running, and then insert the same AppKit host and Ghostty surface under the resulting `AgentsTerminalBodyMountSlotId`.
    */
    if startup_coordinator.startup_readiness_handoff_plan_for_runtime_session(
        true,
        workspace,
        runtime_sessions,
        handoff_plan.runtime_session_id(),
    ) != Some(handoff_plan)
    {
        return false;
    }

    let startup_body_slot_id = handoff_plan.startup_body_slot_id();
    let mount_slot_id = handoff_plan.mount_slot_id;
    let attachment_plan = agents_terminal_attachment_plan_for_startup_handoff(handoff_plan);
    if running_host_native_views.contains_key(&mount_slot_id)
        || running_surface_owners.contains_key(&mount_slot_id)
        || running_config_requests.contains_key(&mount_slot_id)
    {
        return false;
    }

    if !startup_host_native_views
        .get(&startup_body_slot_id)
        .is_some_and(|host_view| {
            host_view.startup_launch_plan() == handoff_plan.startup_launch_plan
                && host_view.can_transfer_to_running_attachment_plan(attachment_plan)
        })
    {
        return false;
    }
    if !startup_surface_owners
        .get(&startup_body_slot_id)
        .is_some_and(|surface| {
            surface.startup_body_slot_id() == startup_body_slot_id
                && surface.runtime_session_id() == handoff_plan.runtime_session_id()
        })
    {
        return false;
    }

    let Some(startup_host_view) = startup_host_native_views.remove(&startup_body_slot_id) else {
        return false;
    };
    let Some(startup_surface_owner) = startup_surface_owners.remove(&startup_body_slot_id) else {
        startup_host_native_views.insert(startup_body_slot_id, startup_host_view);
        return false;
    };

    let changed = startup_coordinator.apply_startup_result(
        workspace,
        runtime_sessions,
        AgentsTerminalStartupResult::Ready {
            completion_intent: handoff_plan.completion_intent,
        },
    );
    if !changed {
        startup_surface_owners.insert(startup_body_slot_id, startup_surface_owner);
        startup_host_native_views.insert(startup_body_slot_id, startup_host_view);
        return false;
    }

    running_mount_slot_bounds.insert(mount_slot_id, handoff_plan.startup_launch_plan.bounds);
    running_host_native_views.insert(
        mount_slot_id,
        startup_host_view.into_running_host_native_view(attachment_plan),
    );
    running_surface_owners.insert(
        mount_slot_id,
        startup_surface_owner.into_running_surface_owner(mount_slot_id),
    );
    startup_config_requests.remove(&startup_body_slot_id);
    startup_body_geometries.remove(&startup_body_slot_id);
    true
}
