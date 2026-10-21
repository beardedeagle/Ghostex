// C1 wave-3 extraction: the terminal launch-payload and close-confirm value types moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

/// Launch strings reach the terminal process as C strings, where an interior NUL would truncate them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LaunchPayloadContainsInteriorNul;

fn check_launch_strings(
    working_directory: Option<&str>,
    command: Option<&str>,
    env_vars: &[(String, String)],
    initial_input: Option<&str>,
) -> Result<(), LaunchPayloadContainsInteriorNul> {
    let env_strings = env_vars
        .iter()
        .flat_map(|(key, value)| [key.as_str(), value.as_str()]);
    let mut strings = working_directory
        .into_iter()
        .chain(command)
        .chain(initial_input)
        .chain(env_strings);
    if strings.any(|value| value.as_bytes().contains(&0)) {
        Err(LaunchPayloadContainsInteriorNul)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalStartupLaunchPayloadSourceKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
}

impl AgentsTerminalStartupLaunchPayloadSourceKey {
    pub(crate) fn from_launch_plan(plan: AgentsTerminalStartupLaunchPlan) -> Self {
        Self {
            runtime_session_id: plan.runtime_session_id,
            shell_session_id: plan.shell_session_id,
            startup_body_slot_id: plan.startup_body_slot_id,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupExplicitLaunchPayload {
    pub(crate) working_directory: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) env_vars: Vec<(String, String)>,
    pub(crate) initial_input: Option<String>,
    pub(crate) wait_after_command: bool,
}

impl AgentsTerminalStartupExplicitLaunchPayload {
    pub(crate) fn validate_launch_strings(&self) -> Result<(), LaunchPayloadContainsInteriorNul> {
        check_launch_strings(
            self.working_directory.as_deref(),
            self.command.as_deref(),
            &self.env_vars,
            self.initial_input.as_deref(),
        )
    }
}

#[derive(Default)]
pub(crate) struct AgentsTerminalStartupLaunchPayloadSource {
    pub(crate) explicit_payloads_by_startup_key: HashMap<
        AgentsTerminalStartupLaunchPayloadSourceKey,
        AgentsTerminalStartupExplicitLaunchPayload,
    >,
}

impl AgentsTerminalStartupLaunchPayloadSource {
    pub(crate) fn new_empty() -> Self {
        Self::default()
    }

    pub(crate) fn insert_explicit_payload_for_startup_key(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        shell_session_id: TerminalSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        payload: AgentsTerminalStartupExplicitLaunchPayload,
    ) {
        self.explicit_payloads_by_startup_key.insert(
            AgentsTerminalStartupLaunchPayloadSourceKey {
                runtime_session_id,
                shell_session_id,
                startup_body_slot_id,
            },
            payload,
        );
    }

    /// Raw payload read for the GPUI-engine startup path, which spawns its
    /// own PTY from the same launch data instead of preparing a Ghostty
    /// surface config. Cleanup stays with
    /// `remove_payload_for_completion_intent` after the startup result is
    /// applied.
    pub(crate) fn explicit_payload_for_launch_plan(
        &self,
        plan: AgentsTerminalStartupLaunchPlan,
    ) -> Option<&AgentsTerminalStartupExplicitLaunchPayload> {
        self.explicit_payloads_by_startup_key
            .get(&AgentsTerminalStartupLaunchPayloadSourceKey::from_launch_plan(plan))
    }

    pub(crate) fn remove_payload_for_completion_intent(
        &mut self,
        completion_intent: AgentsTerminalStartupCompletionIntent,
    ) {
        /*
        CDXC:Terminal 2026-06-23-04:46:
        Failed or stale startup completion must retire exact explicit launch payload data on every platform. The real metadata producer is macOS-only today, but the shared Failed-result boundary must not leave cwd, command, env, or initial-input payloads in runtime memory after the matching startup intent is complete.
        */
        self.explicit_payloads_by_startup_key.remove(
            &AgentsTerminalStartupLaunchPayloadSourceKey {
                runtime_session_id: completion_intent.runtime_session_id,
                shell_session_id: completion_intent.shell_session_id,
                startup_body_slot_id: completion_intent.startup_body_slot_id,
            },
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalLaunchPayloadSourceKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) body_mount_slot_id: AgentsTerminalBodyMountSlotId,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AgentsTerminalExplicitLaunchPayload {
    pub(crate) working_directory: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) env_vars: Vec<(String, String)>,
    pub(crate) initial_input: Option<String>,
    pub(crate) wait_after_command: bool,
}

impl AgentsTerminalExplicitLaunchPayload {
    pub(crate) fn validate_launch_strings(&self) -> Result<(), LaunchPayloadContainsInteriorNul> {
        check_launch_strings(
            self.working_directory.as_deref(),
            self.command.as_deref(),
            &self.env_vars,
            self.initial_input.as_deref(),
        )
    }
}

#[derive(Default)]
pub(crate) struct AgentsTerminalLaunchPayloadSource {
    pub(crate) explicit_payloads_by_agents_key:
        HashMap<AgentsTerminalLaunchPayloadSourceKey, AgentsTerminalExplicitLaunchPayload>,
}

impl AgentsTerminalLaunchPayloadSource {
    pub(crate) fn new_empty() -> Self {
        Self::default()
    }

    pub(crate) fn insert_explicit_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: AgentsTerminalBodyMountSlotId,
        payload: AgentsTerminalExplicitLaunchPayload,
    ) {
        /*
        CDXC:FocusRouting 2026-06-27-13:25:
        Sidebar-attached local gxserver sessions start awake like macOS by feeding the daemon-built attach command directly to the exact Running Agents mount slot. Store it only under the process-local runtime id plus body slot, consume it once during Ghostty config, and never derive launch data from titles, paths, renderer labels, logs, terminal content, or fallback focus.
        */
        self.explicit_payloads_by_agents_key.insert(
            AgentsTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            },
            payload,
        );
    }

    pub(crate) fn has_payload_for_mount_slot(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.explicit_payloads_by_agents_key
            .contains_key(&AgentsTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            })
    }

    /// One-shot drain of the raw payload for the GPUI-engine terminal path,
    /// which spawns its own PTY process from the same launch data instead of
    /// preparing a Ghostty surface config.
    pub(crate) fn take_explicit_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalExplicitLaunchPayload> {
        self.explicit_payloads_by_agents_key
            .remove(&AgentsTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            })
    }

    pub(crate) fn retain_live_mount_slots(
        &mut self,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) {
        self.explicit_payloads_by_agents_key.retain(|key, _| {
            workspace.session_belongs_to_pane(
                key.body_mount_slot_id.pane_id,
                key.body_mount_slot_id.session_id,
            ) && runtime_sessions
                .runtime_session_id_for_shell_session(key.body_mount_slot_id.session_id)
                == Some(key.runtime_session_id)
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CommandTerminalLaunchPayloadSourceKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) body_mount_slot_id: CommandTerminalBodyMountSlotId,
}

impl CommandTerminalLaunchPayloadSourceKey {
    pub(crate) fn from_mount_slot(slot_id: CommandTerminalBodyMountSlotId) -> Self {
        Self {
            runtime_session_id: command_terminal_runtime_session_id(slot_id),
            body_mount_slot_id: slot_id,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CommandTerminalExplicitLaunchPayload {
    pub(crate) working_directory: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) env_vars: Vec<(String, String)>,
    pub(crate) initial_input: Option<String>,
    pub(crate) wait_after_command: bool,
}

impl CommandTerminalExplicitLaunchPayload {
    pub(crate) fn validate_launch_strings(&self) -> Result<(), LaunchPayloadContainsInteriorNul> {
        check_launch_strings(
            self.working_directory.as_deref(),
            self.command.as_deref(),
            &self.env_vars,
            self.initial_input.as_deref(),
        )
    }
}

#[derive(Default)]
pub(crate) struct CommandTerminalLaunchPayloadSource {
    pub(crate) explicit_payloads_by_command_key:
        HashMap<CommandTerminalLaunchPayloadSourceKey, CommandTerminalExplicitLaunchPayload>,
    pending_attach_input_by_session: HashMap<CommandSessionId, String>,
}

impl CommandTerminalLaunchPayloadSource {
    pub(crate) fn new_empty() -> Self {
        Self::default()
    }

    /// CDXC:CommandPane 2026-09-13 WHY:
    /// Clicking an Action while its restored tab is still attaching must retain the command until that attach produces its launch payload.
    pub(crate) fn queue_input_for_pending_attach(
        &mut self,
        session_id: CommandSessionId,
        input: String,
    ) {
        self.pending_attach_input_by_session
            .insert(session_id, input);
    }

    pub(crate) fn insert_explicit_payload_for_mount_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        mut payload: CommandTerminalExplicitLaunchPayload,
    ) {
        /*
        CDXC:Titlebar 2026-06-24-14:24:
        Titlebar terminal Actions are allowed to feed command text only through the command-terminal launch-payload boundary for the exact command-pane mount slot they create. Keep the payload process-local and keyed by command runtime identity plus body slot; do not persist it, log it, infer it from labels/paths, or run it from the titlebar handler.
        */
        if let Some(input) = self
            .pending_attach_input_by_session
            .remove(&slot_id.session_id)
        {
            payload.initial_input = Some(input);
        }
        self.explicit_payloads_by_command_key.insert(
            CommandTerminalLaunchPayloadSourceKey::from_mount_slot(slot_id),
            payload,
        );
    }

    /// One-shot drain of the raw payload for the GPUI-engine terminal path.
    pub(crate) fn take_explicit_payload_for_mount_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> Option<CommandTerminalExplicitLaunchPayload> {
        self.explicit_payloads_by_command_key.remove(
            &CommandTerminalLaunchPayloadSourceKey::from_mount_slot(slot_id),
        )
    }

    pub(crate) fn remove_payloads_for_command_session(&mut self, session_id: CommandSessionId) {
        self.pending_attach_input_by_session.remove(&session_id);
        self.explicit_payloads_by_command_key
            .retain(|key, _| key.body_mount_slot_id.session_id != session_id);
    }

    pub(crate) fn remove_all_payloads(&mut self) {
        self.pending_attach_input_by_session.clear();
        self.explicit_payloads_by_command_key.clear();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalCloseConfirmSurfaceFamily {
    Agents,
    Command,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalCloseConfirmSurfaceSignature {
    pub(crate) title: &'static str,
    pub(crate) message: &'static str,
    pub(crate) keep_open_label: &'static str,
    pub(crate) confirm_action_label: &'static str,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalCloseConfirmDialogKey {
    Agents(AgentsTerminalBodyMountSlotId),
    Command(CommandTerminalBodyMountSlotId),
}

#[cfg(target_os = "macos")]
impl TerminalCloseConfirmDialogKey {
    pub(crate) fn family(self) -> TerminalCloseConfirmSurfaceFamily {
        match self {
            Self::Agents(_) => TerminalCloseConfirmSurfaceFamily::Agents,
            Self::Command(_) => TerminalCloseConfirmSurfaceFamily::Command,
        }
    }
}
