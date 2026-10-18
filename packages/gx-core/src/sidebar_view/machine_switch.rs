//! What a machine switch remembers and restores: the sidebar's machine picker reopens the session
//! a computer was last left on, the way a Space switch reopens a Space's.
//!
//! CDXC:RemoteMachines 2026-10-18 DECISION:
//! User: "when i click on local to switch back to the local machine i want to restore the session i was looking at please (Same thing we have when switching between spaces where we remember what was the last active session/view in each space and we restore it when we switch back needs to apply when switching between machines please)". Every switch of the selected machine (the picker, a reveal, a notification click, a tab correction) first remembers the focused session under its own computer; a switch made with the machine picker then reopens the newest remembered session that computer's list still draws, in the view its project was in. A computer that is offline, or whose remembered sessions are all gone, restores nothing, as before. A switch that brings its own target (a notification click, a reveal) focuses that target instead.
//!
//! The memory is the Space switch's own (`recent_sessions_by_space`, written through
//! `RememberSpaceSession` and stored with the collapse state), under the computer's section key
//! and one reserved slot for the computer as a whole, so it is kept whether or not Spaces are on
//! and survives a restart. The restore is the workspace switch's step
//! ([`plan_workspace_switch_restore`]): the newest remembered row the list draws, else nothing.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/space_switch.rs,
//! apps/desktop/src/app/gx_store/machine_switch.rs.

use super::inputs::{SidebarUiState, LOCAL_MACHINE_ID};
use super::space_switch::{plan_workspace_switch_restore, SpaceSwitchFocus};
use super::view::SidebarView;
use crate::connection::ConnectionPhase;
use crate::core::Core;
use crate::keys::{MachineId, SessionKey};
use crate::sidebar_ui::SidebarUiIntent;

/// The slot of a computer's section that remembers its sessions across all of its Spaces. Real
/// Space ids are generated and never take this form.
pub const MACHINE_SESSION_MEMORY_SPACE_ID: &str = "__machine__";

/// A machine tab id (`local`, or a remote machine's settings id) as the section key the memory is
/// stored under, the same key `SidebarUiState::section_key` gives the selected tab.
fn section_key(machine_tab_id: &str) -> String {
    if machine_tab_id == LOCAL_MACHINE_ID {
        LOCAL_MACHINE_ID.to_string()
    } else {
        format!("remote:{machine_tab_id}")
    }
}

fn machine_tab_id(machine: &MachineId) -> &str {
    machine.remote_id().unwrap_or(LOCAL_MACHINE_ID)
}

/// What a switch to `next_machine_id` remembers first: the focused session, under its own
/// computer. `None` when the switch does not move the selected machine or nothing is focused.
pub fn machine_switch_memory(
    ui: &SidebarUiState,
    focused: Option<&SessionKey>,
    next_machine_id: &str,
) -> Option<SidebarUiIntent> {
    if ui.selected_machine_id == next_machine_id {
        return None;
    }
    let focused = focused?;
    Some(SidebarUiIntent::RememberSpaceSession {
        section_key: section_key(machine_tab_id(&focused.machine)),
        space_id: MACHINE_SESSION_MEMORY_SPACE_ID.to_string(),
        sidebar_session_id: focused.to_sidebar_session_id(),
    })
}

/// The session a machine-picker switch reopens on `machine_id`, from `view`, the list the sidebar
/// draws for it after the switch. `None` (nothing to restore) when the computer is a remote one
/// whose stream is not live, or when none of its remembered sessions is drawn.
pub fn plan_machine_switch_restore(
    core: &Core,
    ui: &SidebarUiState,
    view: &SidebarView,
    machine_id: &str,
) -> Option<SpaceSwitchFocus> {
    if machine_id != LOCAL_MACHINE_ID {
        let machine = MachineId::Remote(machine_id.to_string());
        let live = core
            .presentation()
            .machine(&machine)
            .is_some_and(|entry| entry.connection().phase == ConnectionPhase::Live);
        if !live {
            return None;
        }
    }
    let recent = ui
        .collapse
        .recent_sessions_by_space
        .get(&section_key(machine_id))
        .and_then(|by_space| by_space.get(MACHINE_SESSION_MEMORY_SPACE_ID))?;
    plan_workspace_switch_restore(view, recent)
}
