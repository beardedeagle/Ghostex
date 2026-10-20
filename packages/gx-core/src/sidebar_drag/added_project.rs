//! Where a project the Add Project dialog just added lands.
//!
//! CDXC:Spaces 2026-09-15 DECISION:
//! User: a project added through the Add Project dialog joins the Space that is open in the sidebar
//! and goes to the top of it. Two writes, and they happen at different moments: the MEMBERSHIP is
//! written the instant the dialog reports the project, because the reveal that follows the
//! activation resolves the project's Space and must find it there; the ORDER can only be written
//! once the daemon has listed the project and the sidebar has a group for it, which is one or more
//! snapshots later, so the host holds the project id until then.
//!
//! The membership is written only when the open view is a real Space (never the built-in Other,
//! which claims nothing) and only when the project is not already shown by it: a project the Space
//! already inherits through its Project Group is left alone rather than pinned to the Space
//! directly, which would survive the group moving out of it.
//!
//! SEE-ALSO: the deleted sidebar page's `events.ts`,
//! packages/core-ui/spaces.ts (deleted 2026-10-01) (`addSpaceProjectMember`),
//! packages/core-ui/sidebar-app/drag-drop-geometry.ts (deleted 2026-10-01) (`moveProjectGroupFamilyToStart`),
//! apps/desktop/src/app/gx_store/added_project.rs.

use crate::core::Core;
use crate::keys::MachineId;
use crate::project_docs::{
    toggle_space_member, CollectionsDocument, SpaceMemberKind, SpacesDocument,
};
use crate::sidebar_view::machine_spaces::spaces_enabled_on;
use crate::sidebar_view::spaces::{
    resolve_selected_space, selection_shows_project, SpaceSelection,
};
use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::SidebarInputs;

use super::project_inventory::{project_section, ProjectSection};

/// Whether the sidebar has a group for the added project yet, and what its arrival owes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddedProjectPlacement {
    /// The daemon has not listed the project yet. The host keeps holding it.
    Waiting,
    /// The group is in the list. The host stops holding the project, and writes this order when it
    /// is not the order already held.
    Placed { group_ids: Option<Vec<String>> },
}

/// `addSpaceProjectMember` against the open Space, or `None` when nothing is written.
///
/// `None` covers every `return state` of the TypeScript in one answer, because none of them writes:
/// Spaces off, no document, the built-in Other view, a project the Space already shows, a project
/// id that is blank after the trim, and a Space id the document does not hold.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// The machine is a parameter because `receiveNativeSidebarEvent` reads
/// `describeNativeSidebarMachine(ui, message.remoteMachineId)`, not the selected tab: a project
/// added to a remote machine joins the Space open in THAT machine's section even while the user is
/// looking at this computer's. `message.projectId` is the raw id there, which is what the section's
/// rows and that machine's Spaces document are both keyed by.
pub fn plan_added_project_space_membership(
    core: &Core,
    inputs: &SidebarInputs,
    machine: &MachineId,
    collections: &CollectionsDocument,
    spaces: Option<&SpacesDocument>,
    project_id: &str,
) -> Option<SpacesDocument> {
    let project_id = js_trim(project_id);
    if project_id.is_empty() {
        return None;
    }
    // `nativeSidebarSettings().sidebarSpacesEnabled ? ui.metadata.spaces[machineId] : undefined`,
    // with the switch read off the machine itself (`machine_spaces.rs`).
    if !spaces_enabled_on(core.presentation(), inputs, machine) {
        return None;
    }
    let spaces = spaces?;
    let section = project_section(core, inputs, machine)?;
    let selection = resolve_selected_space(
        &spaces.state,
        inputs
            .ui
            .collapse
            .selected_space_by_section
            .get(&section.section_key)
            .map(String::as_str),
    );
    // `section.selection?.kind === 'space'`: the built-in Other view claims no member, so adding
    // one to it is not a thing the document can express.
    let SpaceSelection::Space(space_id) = &selection else {
        return None;
    };
    let group_id = section
        .rows
        .iter()
        .find(|row| row.project_id.as_deref() == Some(project_id))
        .map(|row| row.group_id.clone());
    // `if (!groupId || !section.isVisible(groupId))`: a project the open Space already shows, by
    // its own membership or through its Project Group, keeps that membership.
    if let Some(group_id) = &group_id {
        if group_visible(&section, collections, spaces, &selection, group_id) {
            return None;
        }
    }
    add_space_project_member(spaces, space_id, project_id)
}

/// Whether the sidebar can place the added project yet, and the order that places it.
///
/// `moveProjectGroupFamilyToStart`: the project, its worktrees and their user-made groups move to
/// the FRONT of this machine's group order, everything else keeping its sequence.
pub fn plan_added_project_placement(
    core: &Core,
    inputs: &SidebarInputs,
    machine: &MachineId,
    project_id: &str,
) -> AddedProjectPlacement {
    let Some(section) = project_section(core, inputs, machine) else {
        return AddedProjectPlacement::Waiting;
    };
    let Some(group_id) = section
        .rows
        .iter()
        .find(|row| row.project_id.as_deref() == Some(project_id))
        .map(|row| row.group_id.clone())
    else {
        return AddedProjectPlacement::Waiting;
    };
    let current = section.group_ids();
    let family: Vec<String> = section.project_ids_of(&section.project_family(&group_id));
    // `projectIds.length > 0 ? [...new Set(projectIds)] : [projectId]`: the family always holds the
    // dragged project itself, so the fallback only matters for a row with no project at all, which
    // cannot be the one just added.
    let is_family = |candidate: &String| {
        section
            .project_of_group(candidate)
            .is_some_and(|project| family.contains(&project))
    };
    let next: Vec<String> = current
        .iter()
        .filter(|group_id| is_family(group_id))
        .chain(current.iter().filter(|group_id| !is_family(group_id)))
        .cloned()
        .collect();
    AddedProjectPlacement::Placed {
        // `if (groupIds.some((id, index) => id !== section.groupIds[index]))`: an order that did not
        // move posts nothing, which is the one place in the project family where an unchanged write
        // really is skipped.
        group_ids: (next != current).then_some(next),
    }
}

/// `addSpaceProjectMember`: a project that is already a direct member, or a Space the document does
/// not hold, leaves the document alone.
fn add_space_project_member(
    document: &SpacesDocument,
    space_id: &str,
    project_id: &str,
) -> Option<SpacesDocument> {
    // `state.spaces[spaceId]?.memberProjectIds.includes(trimmed) !== false` is `true` for BOTH an
    // unknown Space (`undefined !== false`) and a project already in it, and either way the
    // TypeScript returns the state untouched.
    let space = document.state.spaces.get(space_id)?;
    if space
        .member_project_ids
        .iter()
        .any(|member| member == project_id)
    {
        return None;
    }
    Some(toggle_space_member(
        document,
        space_id,
        SpaceMemberKind::Project,
        project_id,
    ))
}

/// `createSelectedSidebarSpaceVisibility(...)(groupId)`.
fn group_visible(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    spaces: &SpacesDocument,
    selection: &SpaceSelection,
    group_id: &str,
) -> bool {
    let Some(row) = section.row(group_id) else {
        return false;
    };
    let collection_id = row
        .project_id
        .as_deref()
        .and_then(|project_id| section.collection_of_project(collections, project_id));
    selection_shows_project(
        selection,
        &spaces.state,
        row.project_id.as_deref(),
        collection_id.as_deref(),
        row.parent_project_id.as_deref(),
    )
}
