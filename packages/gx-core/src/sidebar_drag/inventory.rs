//! The list a move is computed against, which is NOT the list the sidebar draws.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! A DRAWN INDEX IS NOT A STORED INDEX. `reorderNativeSidebar` reads `sidebarStore`'s
//! `sessionIdsByGroup`, which is the PROJECTION's membership: every row the projection built for a
//! group, in the group's own order, before the sidebar sorts it into sections, before the tag
//! filter, and before a Space or Show Hidden takes anything off screen. The drawn list
//! (`GroupCore::sessions`) is that list sorted and filtered, so a drag computed against it would
//! insert a row at a different place whenever a filter hid a row between the source and the
//! destination, and the saved order would not be the order the user saw. This is the same defect
//! class as "Copy Path resolves against the drawn list", which has bitten this port three times, so
//! the membership is rebuilt here from the same `project_members` the view model builds its group
//! plans from rather than read off the view.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/membership.rs,
//! packages/core-ui/sidebar-store-model.ts (deleted 2026-10-01) (`sessionIdsByGroup`, `sessionsById`, `groupsById`).

use crate::core::Core;
use crate::keys::{parse_workspace_subgroup_id, MachineId, ProjectKey, SessionKey};
use crate::sidebar_view::membership::project_members;
use crate::sidebar_view::projects::build_project_meta;
use crate::sidebar_view::SidebarInputs;

/// One row of a group's membership, with the two facts a move reads off it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveRow {
    pub(crate) sidebar_session_id: String,
    /// `sessionsById[id].isPinned`, which decides which of the two move rules applies.
    pub(crate) is_pinned: bool,
}

/// A group the projection built, named the way `groupsById` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveGroup {
    pub(crate) group_id: String,
    pub(crate) machine: MachineId,
    pub(crate) project: Option<ProjectKey>,
    pub(crate) rows: Vec<MoveRow>,
}

impl MoveGroup {
    pub(crate) fn session_ids(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| row.sidebar_session_id.clone())
            .collect()
    }

    pub(crate) fn index_of(&self, sidebar_session_id: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.sidebar_session_id == sidebar_session_id)
    }
}

/// The rows of a group, in the order a move is computed against. Public so a probe can record the
/// list the planner really used rather than describe one beside it: a record that can drift from
/// the list that decided is a record that agrees with any answer.
pub fn sidebar_group_membership(
    core: &Core,
    inputs: &SidebarInputs,
    group_id: &str,
) -> Option<Vec<String>> {
    group_by_id(core, inputs, group_id).map(|group| group.session_ids())
}

/// The group with this id, as the projection built it, or `None` for a group it has no row for.
///
/// `None` is what `state.groupsById[id]` being absent means, and the TypeScript's answer to that is
/// to do nothing at all.
pub(crate) fn group_by_id(
    core: &Core,
    inputs: &SidebarInputs,
    group_id: &str,
) -> Option<MoveGroup> {
    if let Some((project, subgroup_id)) = parse_workspace_subgroup_id(group_id) {
        return subgroup_membership(core, inputs, &project, &subgroup_id);
    }
    let project = ProjectKey::parse_sidebar_group_id(group_id)?;
    project_membership(core, inputs, &project)
}

/// The group that holds a session, which is `findSessionGroupId(sessionIdsByGroup, sessionId)`.
///
/// The projection is a partition: a row taken by a user-made group is not in its project's own
/// list, and a row the projection filtered out (not listed in the sidebar by default, on the
/// commands surface, hidden locally) is in NO group at all, which is the `undefined` the TypeScript
/// returns before it does anything.
pub(crate) fn group_of_session(
    core: &Core,
    inputs: &SidebarInputs,
    sidebar_session_id: &str,
) -> Option<MoveGroup> {
    let session = SessionKey::parse_sidebar_session_id(sidebar_session_id)?;
    let project = session.project_key();
    let document_group = core
        .presentation()
        .machine(&MachineId::Local)
        .and_then(|machine| machine.side_state().workspace_groups.as_ref())
        .and_then(|state| state.projects.get(&project.to_workspace_project_id()))
        .and_then(|groups| {
            groups
                .groups
                .iter()
                .find(|group| group.session_ids.contains(&session.session_id))
        })
        .map(|group| group.group_id.clone());
    let candidate = match document_group {
        Some(group_id) => subgroup_membership(core, inputs, &project, &group_id),
        None => project_membership(core, inputs, &project),
    }?;
    candidate
        .index_of(sidebar_session_id)
        .is_some()
        .then_some(candidate)
}

/// A project group, or `None` for a project the projection draws no group for (parked, recent,
/// or locally hidden).
fn project_membership(
    core: &Core,
    inputs: &SidebarInputs,
    project: &ProjectKey,
) -> Option<MoveGroup> {
    let store = core.presentation();
    let entry = store.machine(&project.machine)?;
    let meta = build_project_meta(
        entry,
        workspace_project_order(core, &project.machine),
        inputs.host.parked_project_ids(&project.machine),
    );
    if !meta
        .grouped_project_ids()
        .any(|project_id| *project_id == project.project_id)
    {
        return None;
    }
    let members = project_members(store, entry, &project.machine, &project.project_id);
    Some(MoveGroup {
        group_id: project.to_sidebar_group_id(),
        machine: project.machine.clone(),
        project: Some(project.clone()),
        // The app's browser tabs come FIRST in a project group's rows. The desktop host feeds none
        // since 2026-09-20 (they moved to the view panel's tab strip), and the two places their
        // absence could change an answer are refused by name in `session_move.rs` rather than
        // guessed at here.
        rows: members
            .session_ids
            .iter()
            .filter_map(|session_id| move_row(core, project, session_id))
            .collect(),
    })
}

/// One user-made group of a project.
fn subgroup_membership(
    core: &Core,
    inputs: &SidebarInputs,
    project: &ProjectKey,
    subgroup_id: &str,
) -> Option<MoveGroup> {
    let store = core.presentation();
    let entry = store.machine(&project.machine)?;
    let meta = build_project_meta(
        entry,
        workspace_project_order(core, &project.machine),
        inputs.host.parked_project_ids(&project.machine),
    );
    // A user-made group is drawn under its project, so a project the list has no row for has no
    // subgroup rows either. A CHAT project's user-made groups are never drawn at all.
    if !meta
        .grouped_project_ids()
        .any(|project_id| *project_id == project.project_id)
    {
        return None;
    }
    let members = project_members(store, entry, &project.machine, &project.project_id);
    let subgroup = members.subgroups.iter().find(|group| {
        group.sidebar_group_id == crate::keys::encode_workspace_subgroup_id(project, subgroup_id)
    })?;
    Some(MoveGroup {
        group_id: subgroup.sidebar_group_id.clone(),
        machine: project.machine.clone(),
        project: Some(project.clone()),
        rows: subgroup
            .session_ids
            .iter()
            .filter_map(|session_id| move_row(core, project, session_id))
            .collect(),
    })
}

fn move_row(core: &Core, project: &ProjectKey, session_id: &str) -> Option<MoveRow> {
    let row = core
        .presentation()
        .machine(&project.machine)?
        .effective_session(&project.project_id, session_id)?;
    Some(MoveRow {
        is_pinned: row.is_pinned,
        sidebar_session_id: SessionKey {
            machine: project.machine.clone(),
            project_id: project.project_id.clone(),
            session_id: session_id.to_string(),
        }
        .to_sidebar_session_id(),
    })
}

/// The manual project order the projection reads, which is the local document's `projectOrder`.
pub(super) fn workspace_project_order<'a>(core: &'a Core, machine: &MachineId) -> &'a [String] {
    core.presentation()
        .machine(machine)
        .and_then(|entry| entry.side_state().workspace_groups.as_ref())
        .map(|groups| groups.project_order.as_slice())
        .unwrap_or_default()
}
