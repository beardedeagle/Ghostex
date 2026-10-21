// C1 wave-3 re-cluster: focused mount-slot/runtime-session-id lookups, sleeping placeholder wake targets and close-confirm text, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn focused_agents_terminal_surface_mount_slot(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
) -> Option<AgentsTerminalBodyMountSlotId> {
    /*
    CDXC:Terminal 2026-06-22-22:59:
    Real Agents Ghostty surface focus is a runtime decision derived from shell focus only: Agents mode, an AgentsPane focus target, a rendered pane, and that pane's selected Running session. Command pane, Browser/project-editor modes, sleeping or hidden Focus-mode panes, stale panes, and missing sessions must leave every mounted terminal surface unfocused without adding input routing or persisted focus ids.
    */
    let _ = active_mode;
    let ShellFocusTarget::AgentsPane(focused_pane_id) = shell_focus else {
        return None;
    };

    agents_workspace
        .rendered_terminal_body_mount_slots()
        .into_iter()
        .find(|slot_id| slot_id.pane_id == focused_pane_id)
}

pub(crate) fn command_terminal_runtime_session_id(
    slot_id: CommandTerminalBodyMountSlotId,
) -> AgentsTerminalRuntimeSessionId {
    /*
    CDXC:Terminal 2026-06-23-05:03:
    Command-pane surfaces need a process-local owner identity for the shared Ghostty owner, but they must not enter the Agents runtime-session registry or any startup map. Derive this transient id only from the command mount slot and keep it scoped to the command surface owner map.
    */
    const COMMAND_RUNTIME_ID_NAMESPACE: u64 = 0xC000_0000_0000_0000;
    AgentsTerminalRuntimeSessionId(
        COMMAND_RUNTIME_ID_NAMESPACE
            | ((slot_id.group_id.0 & 0x7FFF_FFFF) << 32)
            | (slot_id.session_id.0 & 0xFFFF_FFFF),
    )
}

pub(crate) fn command_terminal_runtime_session_id_from_gxserver_key(
    key: &GpuiLocalWorkspaceSessionKey,
) -> AgentsTerminalRuntimeSessionId {
    /*
    CDXC:CommandPane 2026-07-04:
    GPUI-engine command terminals use the daemon project/session identity as
    their runtime owner key once gxserver creation succeeds. Keep the value
    process-local and numeric for existing terminal runtime maps, but derive it
    only from the real gxserver ids instead of the command-pane mount slot.
    */
    const COMMAND_GXSERVER_RUNTIME_ID_NAMESPACE: u64 = 0xD000_0000_0000_0000;
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in key
        .project_id
        .bytes()
        .chain([b':'])
        .chain(key.session_id.bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    AgentsTerminalRuntimeSessionId(
        COMMAND_GXSERVER_RUNTIME_ID_NAMESPACE | (hash & 0x0FFF_FFFF_FFFF_FFFF),
    )
}

pub(crate) fn command_terminal_runtime_session_id_from_remote_reference(
    reference: &GpuiRemoteAttachSessionReference,
) -> AgentsTerminalRuntimeSessionId {
    /*
    CDXC:RemoteMachines 2026-08-29:
    A remote Action's command tab has no local daemon identity, so it derives
    its runtime owner from the remote machine/project/session triple instead.
    That keeps the id stable while the tab is moved between command groups —
    unlike the mount-slot form — and it can never collide with a local
    command-surface id because it owns its own namespace.
    */
    const COMMAND_REMOTE_RUNTIME_ID_NAMESPACE: u64 = 0xE000_0000_0000_0000;
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in reference
        .remote_machine_id
        .bytes()
        .chain([b':'])
        .chain(reference.project_id.bytes())
        .chain([b':'])
        .chain(reference.session_id.bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    AgentsTerminalRuntimeSessionId(
        COMMAND_REMOTE_RUNTIME_ID_NAMESPACE | (hash & 0x0FFF_FFFF_FFFF_FFFF),
    )
}

pub(crate) fn focused_command_terminal_surface_mount_slot(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<CommandTerminalBodyMountSlotId> {
    /*
    CDXC:Terminal 2026-06-23-05:03:
    Command Ghostty focus is mirrored only when shell focus is the command pane and the focused command group has a mounted active session. Agents, Browser, project-editor focus, collapsed panes, inactive command tabs, and missing sessions clear command focus without synthetic input routing.
    */
    if shell_focus != ShellFocusTarget::CommandPane {
        return None;
    }
    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    let slot_id = CommandTerminalBodyMountSlotId {
        group_id,
        session_id,
    };
    command_pane
        .is_current_terminal_body_mount_slot(slot_id)
        .then_some(slot_id)
}

/// Press-any-key wake for sleeping Agents bodies mirrors the command-pane
/// rule: only the visible focused pane's selected sleeping tab wakes, and only
/// from plain alphanumeric keys; Focus-mode-hidden panes have no visible
/// placeholder and stay parked.
pub(crate) fn focused_sleeping_agents_placeholder_wake_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
    keystroke: &Keystroke,
) -> Option<(WorkspacePaneId, TerminalSessionId)> {
    let _ = active_mode;
    if !command_pane_sleeping_placeholder_keystroke_requests_wake(keystroke) {
        return None;
    }
    let ShellFocusTarget::AgentsPane(pane_id) = shell_focus else {
        return None;
    };
    if !agents_workspace.rendered_leaf_order().contains(&pane_id) {
        return None;
    }
    let leaf = agents_workspace.find_leaf(pane_id)?;
    let session_id = leaf.tab_group.active_tab;
    agents_workspace
        .session(session_id)
        .is_some_and(|session| {
            session.presentation_state == TerminalSessionPresentationState::Sleeping
        })
        .then_some((pane_id, session_id))
}

pub(crate) fn focused_sleeping_command_placeholder_wake_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
    keystroke: &Keystroke,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:SessionSleep 2026-06-25-14:49:
    Keyboard wake is scoped to the command pane's focused active tab and only when that command session is parked sleeping. Non-command focus, running command terminals, collapsed/missing groups, and non-alphanumeric keys must not create terminals, reroute input, or mutate shell state.

    CDXC:SessionSleep 2026-06-25-19:07:
    Native key wake belongs to the visible AppKit placeholder first responder. A collapsed command strip may remember command focus, but it has no visible placeholder body, so alphanumeric keys must not wake parked command tabs until the panel is expanded.

    CDXC:SessionSleep 2026-06-27-04:36:
    Keyboard wake resolves through the visible command body owner, so only the exact focused selected sleeping tab can wake. Stale selected ids, missing sessions, inactive siblings, and collapsed panes have no visible placeholder owner and must not wake or create a terminal.
    */
    if shell_focus != ShellFocusTarget::CommandPane
        || !command_pane.focused_group_dock_visible()
        || !command_pane_sleeping_placeholder_keystroke_requests_wake(keystroke)
    {
        return None;
    }

    let leaf = command_pane.find_leaf(command_pane.focused_group)?;
    let body_owner = command_pane.visible_command_body_owner_for_leaf(leaf)?;
    body_owner
        .is_sleeping
        .then_some((body_owner.group_id, body_owner.session_id))
}

pub(crate) fn terminal_close_confirm_surface_signature(
    family: TerminalCloseConfirmSurfaceFamily,
) -> TerminalCloseConfirmSurfaceSignature {
    /*
    CDXC:CommandPane 2026-06-23-20:04:
    Close-confirm UI copy must stay generic while the action is enabled by the real GhosttyKit `needs_confirm_quit` ABI contract. It may identify only the safe family scope, Keep Open cancel action, and generic close action; it must not display session names, terminal titles, command text, paths, URLs, stdout/stderr, terminal content, runtime ids, tokens, raw callback payloads, or fallback close behavior.
    */
    let message = match family {
        TerminalCloseConfirmSurfaceFamily::Agents => {
            "An Agents terminal is asking for confirmation before closing."
        }
        TerminalCloseConfirmSurfaceFamily::Command => {
            "A command terminal is asking for confirmation before closing."
        }
    };

    TerminalCloseConfirmSurfaceSignature {
        title: "Terminal close requested",
        message,
        keep_open_label: "Keep Open",
        confirm_action_label: "Close Terminal",
    }
}

pub(crate) fn terminal_session_title_for_id(_id: TerminalSessionId) -> String {
    "Terminal Session".to_string()
}

pub(crate) fn command_session_title_for_id(_id: CommandSessionId) -> String {
    /*
    CDXC:CommandPane 2026-06-25-11:56:
    Restored GPUI command placeholders must use the same generic `Command Terminal` fallback as newly created macOS command-pane terminals. Do not derive fallback titles from command ids or persist visible/private command titles, because shell-state JSON is layout metadata rather than command-content storage.
    */
    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string()
}

pub(crate) fn gpui_command_pane_sidebar_indicator_text(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()
        && normalized.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !normalized.chars().any(char::is_control))
    .then_some(normalized)
}
