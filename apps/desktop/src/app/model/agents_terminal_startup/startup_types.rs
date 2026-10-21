use super::*;
use crate::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalStartupBodySlotId {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupBodyGeometry {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupLaunchPlan {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupRecord {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_geometry_available: bool,
}

impl AgentsTerminalStartupRecord {
    pub(crate) fn startup_body_slot_id(self) -> AgentsTerminalStartupBodySlotId {
        AgentsTerminalStartupBodySlotId {
            pane_id: self.pane_id,
            session_id: self.shell_session_id,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupCompletionIntent {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
}

impl AgentsTerminalStartupCompletionIntent {
    pub(crate) fn from_record(
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        record: AgentsTerminalStartupRecord,
    ) -> Self {
        Self {
            runtime_session_id,
            shell_session_id: record.shell_session_id,
            startup_body_slot_id: record.startup_body_slot_id(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AgentsTerminalStartupCompletionSignal {
    Ready {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
    Failed {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
}

impl AgentsTerminalStartupCompletionSignal {
    pub(crate) fn completion_intent(self) -> AgentsTerminalStartupCompletionIntent {
        match self {
            Self::Ready { completion_intent } | Self::Failed { completion_intent } => {
                completion_intent
            }
        }
    }

    pub(crate) fn into_startup_result(self) -> AgentsTerminalStartupResult {
        match self {
            Self::Ready { completion_intent } => {
                AgentsTerminalStartupResult::Ready { completion_intent }
            }
            Self::Failed { completion_intent } => {
                AgentsTerminalStartupResult::Failed { completion_intent }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AgentsTerminalStartupResult {
    Ready {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
    Failed {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
}

impl AgentsTerminalStartupResult {
    pub(crate) fn completion_intent(self) -> AgentsTerminalStartupCompletionIntent {
        match self {
            Self::Ready { completion_intent } | Self::Failed { completion_intent } => {
                completion_intent
            }
        }
    }

    pub(crate) fn runtime_session_id(self) -> AgentsTerminalRuntimeSessionId {
        self.completion_intent().runtime_session_id
    }

    pub(crate) fn terminal_presentation_state(self) -> TerminalSessionPresentationState {
        match self {
            Self::Ready { .. } => TerminalSessionPresentationState::Running,
            Self::Failed { .. } => TerminalSessionPresentationState::StartupFailed,
        }
    }
}
