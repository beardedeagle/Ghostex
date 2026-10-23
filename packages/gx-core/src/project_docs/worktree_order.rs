//! Worktrees nest under their parent project, and a project drag carries its family with it.
//!
//! CDXC:Worktrees 2026-09-21 WHY:
//! `packages/shared/project-worktree-order.ts` (deleted 2026-10-01) is generic over an ITEM (`{orderId, projectId,
//! worktree}`) and the sidebar list only ever needed it over PROJECT IDS, so M4a ported the
//! id-shaped half into `sidebar_view/projects.rs`. The project moves need the item-shaped half,
//! because a drag is keyed by GROUP id and several groups (a project and each of its user-made
//! groups) share one project id. Writing the second half beside the first is how this port's twin
//! bugs start, so the generic lives here and the list's ordering calls it with items built from its
//! own metadata. The user drags worktrees constantly, so a difference between "how the list nests
//! them" and "where a drop puts them" would be visible immediately and attributed to the drag.
//!
//! SEE-ALSO: packages/shared/project-worktree-order.ts (deleted 2026-10-01),
//! packages/gx-core/src/sidebar_view/projects.rs, packages/gx-core/src/sidebar_drag/project_move.rs.

use std::collections::BTreeSet;

use crate::sidebar_view::text::js_trim;

/// One row of the order. `order_id` is what a drop names; `project_id` is what a worktree points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectOrderItem {
    /// `getProjectOrderId`: the item's own `orderId`, falling back to its project id.
    pub order_id: String,
    pub project_id: String,
    /// `worktree.parentProjectId`, exactly as the row carries it. The trim is applied HERE rather
    /// than by the caller, because the TypeScript trims at every read and an all-spaces value is
    /// not a parent there either; a caller that trimmed first would be a second rule to keep in
    /// step, and `js_trim` is not `str::trim`.
    pub parent_project_id: Option<String>,
}

impl ProjectOrderItem {
    fn parent(&self) -> Option<&str> {
        self.parent_project_id
            .as_deref()
            .map(js_trim)
            .filter(|parent| !parent.is_empty())
    }
}

/// Where a drop lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPosition {
    Before,
    After,
}

impl DropPosition {
    /// `position === 'after' ? 1 : 0`, which is how every insert index in this family is built.
    fn offset(self) -> usize {
        match self {
            Self::After => 1,
            Self::Before => 0,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "after" => Some(Self::After),
            "before" => Some(Self::Before),
            _ => None,
        }
    }
}

/// `moveProjectsWithWorktrees`: the drop, then a re-nest of the whole order.
///
/// A drop the rules forbid returns the order UNCHANGED rather than refusing, which is what the
/// TypeScript's `[...projects]` does, and what makes the caller post an order that did not move.
pub fn move_projects_with_worktrees(
    projects: &[ProjectOrderItem],
    source_order_id: &str,
    target_order_id: &str,
    position: DropPosition,
) -> Vec<ProjectOrderItem> {
    if !can_drop_project_with_worktrees(projects, source_order_id, target_order_id, position) {
        return projects.to_vec();
    }
    let source_index = projects
        .iter()
        .position(|project| project.order_id == source_order_id);
    let target_index = projects
        .iter()
        .position(|project| project.order_id == target_order_id);
    let (Some(source_index), Some(target_index)) = (source_index, target_index) else {
        return projects.to_vec();
    };
    if source_index == target_index {
        return projects.to_vec();
    }
    let insert_index = target_index + position.offset();
    let adjusted = match insert_index > source_index {
        true => insert_index - 1,
        false => insert_index,
    };
    let mut next: Vec<ProjectOrderItem> = projects
        .iter()
        .filter(|project| project.order_id != source_order_id)
        .cloned()
        .collect();
    next.insert(adjusted.min(next.len()), projects[source_index].clone());
    order_projects_with_worktrees(&next)
}

/// `canDropProjectWithWorktrees`: a main project may go anywhere except inside its own family, and
/// a worktree may only move inside the family it belongs to.
pub fn can_drop_project_with_worktrees(
    projects: &[ProjectOrderItem],
    source_order_id: &str,
    target_order_id: &str,
    position: DropPosition,
) -> bool {
    let by_order_id = |order_id: &str| last_by(projects, |project| project.order_id == order_id);
    let (Some(source), Some(target)) = (by_order_id(source_order_id), by_order_id(target_order_id))
    else {
        return false;
    };
    if source_order_id == target_order_id {
        return false;
    }
    let source_family = family_parent_id(projects, &source.project_id);
    let target_family = family_parent_id(projects, &target.project_id);
    let Some(source_family) = source_family else {
        // A main project: anywhere except onto one of its own worktrees.
        return target_family.as_deref() != Some(source.project_id.as_str());
    };
    if target.project_id == source_family {
        // Onto its own parent, which is only meaningful below it.
        return position == DropPosition::After;
    }
    target_family.as_deref() == Some(source_family.as_str())
}

/// `orderProjectsWithWorktrees`: the projects with their worktrees nested under their parents.
///
/// The worktree set is keyed by PROJECT id, not by order id, which is what makes several rows of
/// one project (its own group and each of its user-made groups) move together as one block.
pub fn order_projects_with_worktrees(projects: &[ProjectOrderItem]) -> Vec<ProjectOrderItem> {
    let mut worktree_project_ids: BTreeSet<String> = BTreeSet::new();
    let mut worktrees_by_parent: Vec<(String, Vec<ProjectOrderItem>)> = Vec::new();
    for project in projects {
        let Some(family) = family_parent_id(projects, &project.project_id) else {
            continue;
        };
        if !projects
            .iter()
            .any(|candidate| candidate.project_id == family)
        {
            continue;
        }
        worktree_project_ids.insert(project.project_id.clone());
        match worktrees_by_parent
            .iter_mut()
            .find(|(parent, _)| *parent == family)
        {
            Some((_, items)) => items.push(project.clone()),
            None => worktrees_by_parent.push((family, vec![project.clone()])),
        }
    }
    let mut ordered: Vec<ProjectOrderItem> = Vec::with_capacity(projects.len());
    for project in projects {
        if worktree_project_ids.contains(&project.project_id) {
            continue;
        }
        ordered.push(project.clone());
        // A project that is ITSELF a worktree (of a parent the order does not hold) does not
        // collect children: the TypeScript asks its own `worktree.parentProjectId` here, not the
        // resolved family, so a chain whose top is missing keeps its links flat.
        if project.parent().is_none() {
            if let Some((_, items)) = worktrees_by_parent
                .iter()
                .find(|(parent, _)| *parent == project.project_id)
            {
                ordered.extend(items.iter().cloned());
            }
        }
    }
    ordered
}

/// `resolveProjectWorktreeFamilyParentId`: walk up to the top of the chain, and fall back to the
/// DIRECT parent when the chain loops.
fn family_parent_id(projects: &[ProjectOrderItem], project_id: &str) -> Option<String> {
    let parent_of = |project_id: &str| -> Option<String> {
        last_by(projects, |project| project.project_id == project_id)
            .and_then(|project| project.parent().map(str::to_string))
    };
    let direct = parent_of(project_id)?;
    let mut family = direct.clone();
    let mut seen: BTreeSet<String> = BTreeSet::from([project_id.to_string()]);
    while !seen.contains(&family) {
        seen.insert(family.clone());
        match parent_of(&family) {
            Some(next) => family = next,
            None => return Some(family),
        }
    }
    Some(direct)
}

/// `new Map(projects.map(...))`, where a later entry REPLACES an earlier one with the same key.
/// Several rows share a project id, so which one answers is not an implementation detail.
fn last_by(
    projects: &[ProjectOrderItem],
    matches: impl Fn(&ProjectOrderItem) -> bool,
) -> Option<&ProjectOrderItem> {
    projects.iter().rev().find(|project| matches(project))
}
