//! The project slot hotkeys, cmd+ctrl+1 to cmd+ctrl+9 by default: the port of `runNativeProjectSlotHotkey`.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! This one is not a sidebar command and never was. It arrives as `gpuiProjectSlotHotkey`, a THIRD
//! route beside the two sidebar-command envelopes, and it reaches into the collapse state and
//! DELETES the project's collapsed flag directly.
//!
//! The whole jump is planned here: which project the slot names, what it does to that project's
//! collapse state, which row it focuses and whether that row is revealed. The reveal itself is
//! `reveal_plan`, asked by the host AFTER this plan's own intents are applied, which is the order the
//! TypeScript has (the deletion, then the selection, then the reveal at the next build). Supersedes
//! the 2026-09-21 note that the focus and the reveal stayed with the old runtime.
//!
//! **A slot never names a remote project.** The TypeScript indexes `!group.remoteMachineContext`
//! groups of the list it draws, and that list only ever holds the SELECTED machine's groups, so on
//! a remote machine's tab (connected, last-seen or not loaded) the slot finds nothing and the key
//! does nothing at all: no deletion, no selection, no reveal. The remote focus machinery is
//! therefore never reached from here, and nothing here builds a second one.
//!
//! DELETE, not toggle. `runNativeProjectSlotHotkey` removes the key, so a jump to a project that
//! is already expanded leaves it expanded, where a toggle would collapse it.
//!
//! SEE-ALSO: the deleted sidebar page's `hotkeys.ts` (`runNativeProjectSlotHotkey`),
//! packages/gx-core/src/sidebar_ui/intents.rs (`ExpandProjectForSlotJump`),
//! apps/desktop/src/app/gx_store/sidebar_slot_jump.rs.

use super::inputs::{SidebarSettings, SidebarUiState};
use super::view::SidebarView;
use crate::sidebar_ui::SidebarUiIntent;

/// What a slot hotkey does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSlotPlan {
    pub group_id: String,
    /// The group was collapsed when the key was pressed, which is the TypeScript's `collapsed` and
    /// decides both branches below.
    pub was_collapsed: bool,
    /// Expand the group: it was collapsed and `expandCollapsedProjectsOnJump` is on.
    pub expand_group: bool,
    /// The row the jump focuses: the group's focused row, else its first, in the order the group
    /// holds them (`group.sessions.find(isFocused) ?? group.sessions[0]`, which is the list order
    /// and not the drawn heading order). `None` for a project with no row, where the TypeScript
    /// selects nothing, and the multi-selection is then left alone too: it is cleared by that
    /// selection, not by the jump.
    pub target_session_id: Option<String>,
    /// The target is revealed: `if (!collapsed || expandCollapsedProjectsOnJump)`. A collapsed
    /// project with the setting off is focused and left collapsed.
    pub reveal: bool,
}

impl ProjectSlotPlan {
    /// The changes this plan makes to the sidebar's own state, in the TypeScript's order. The host
    /// and the gate apply exactly these.
    pub fn intents(&self) -> Vec<SidebarUiIntent> {
        let mut intents = Vec::new();
        if self.target_session_id.is_some() {
            intents.push(SidebarUiIntent::SetSelectedSessions {
                session_ids: Vec::new(),
            });
        }
        if self.expand_group {
            intents.push(SidebarUiIntent::ExpandProjectForSlotJump {
                group_id: self.group_id.clone(),
            });
        }
        intents
    }
}

/// The project a slot names, and what the jump does.
///
/// `None` for a slot outside 1 to 9 and for a slot with no project behind it, which is what the
/// TypeScript's two early returns are: the Nth LOCAL group of the drawn list, and only when that
/// group is a project (a user-made group is not).
///
/// The cost is one pass over the drawn groups and one over the named group's rows; nothing is
/// built.
pub fn project_slot_plan(
    view: &SidebarView,
    ui: &SidebarUiState,
    settings: &SidebarSettings,
    slot_number: u32,
) -> Option<ProjectSlotPlan> {
    if !(1..=9).contains(&slot_number) {
        return None;
    }
    let group = view
        .groups
        .iter()
        // `!group.remoteMachineContext`: this computer's groups, in the order the list draws them.
        .filter(|group| group.core.remote_machine.is_none())
        .nth(slot_number as usize - 1)?;
    group.core.project_context.as_ref()?;
    let was_collapsed = ui.collapse.collapsed_groups.contains(&group.core.group_id);
    let expand_group = was_collapsed && settings.expand_collapsed_projects_on_jump;
    let sessions = &group.core.sessions;
    let target_session_id = sessions
        .iter()
        .find(|session| session.is_focused)
        .or_else(|| sessions.first())
        .map(|session| session.row.sidebar_session_id.clone());
    Some(ProjectSlotPlan {
        group_id: group.core.group_id.clone(),
        was_collapsed,
        expand_group,
        reveal: target_session_id.is_some()
            && (!was_collapsed || settings.expand_collapsed_projects_on_jump),
        target_session_id,
    })
}
