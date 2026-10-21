// C1 wave-3 re-cluster: terminal body mount-slot id types and the shared TerminalSurfaceMountSlotKey trait, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

/*
CDXC:Terminal 2026-06-23-20:34:
Focused terminal text mount targets derive Debug for compile-time diagnostics and tests, so their Agents and command slot IDs must carry Debug too. The derived output contains only stable numeric IDs, not user-owned terminal text or paths.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalBodyMountSlotId {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandTerminalBodyMountSlotId {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}

/*
CDXC:Zmx 2026-07-06:
Runtime-only identity of the terminal slot that currently owns shell focus,
compared across renders to mirror macOS
`refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged`. Carries only slot ids;
never persisted or logged with titles, paths, or terminal content.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZmxPersistenceFocusedTerminalSlot {
    Agents(AgentsTerminalBodyMountSlotId),
    Command(CommandTerminalBodyMountSlotId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneVisibleBodyOwner {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
    pub(crate) is_sleeping: bool,
}

impl CommandPaneVisibleBodyOwner {
    pub(crate) fn mount_slot_id(self) -> Option<CommandTerminalBodyMountSlotId> {
        (!self.is_sleeping).then_some(CommandTerminalBodyMountSlotId {
            group_id: self.group_id,
            session_id: self.session_id,
        })
    }
}
