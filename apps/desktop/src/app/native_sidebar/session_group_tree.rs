//! A project's user-made session groups, drawn inside the project's block: after its own rows,
//! indented, with a tree line from under the project's chevron down to the last group's header.
//!
//! CDXC:Projects 2026-10-23 DECISION:
//! Agreed with the user on a call (PR #286): one look for session groups, indented under their project, with no setting. A session group used to be drawn at its project's level with the same header and the same letter tile, so with a few projects it was not clear which group belonged to which project. Indenting the groups under the project with a tree line (the line a coordinator's threads hang from, threads.rs) makes them read as part of it. They sit inside the project's fold rather than as list blocks of their own, so collapsing or expanding the project moves them in the same motion instead of making them vanish or appear at once. A framed look, a project-name label and a setting to pick between them were tried and dropped: one look is less to maintain, and a frame clashes with projects that sit in a project group.

use std::collections::{HashMap, HashSet};

use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};
use gpui_component::v_flex;

use serde_json::Value;

use super::appearance::SidebarAppearance;
use super::model::{NativeSidebarGroup, NativeSidebarSnapshot};
use crate::GhostexGpuiApp;
use crate::app::helpers::chrome_color;

/// How far a session group sits right of its project.
const CHILD_INDENT: f32 = 14.0;
/// The gap above each session group.
const GROUP_GAP: f32 = 2.0;
/// The project header box reaches 15px left of its block (project_header.rs,
/// `PROJECT_HEADER_CHEVRON_INSET`) and its chevron is 16px wide, so the chevron's centre is 7px
/// left of the block. The 1px line is centred on it.
const LINE_X: f32 = -7.5;
/// How far the project's fold reaches left of its block, so the line sits inside the box the fold
/// clips to while it moves (GPUI clips both axes of an `overflow_hidden` box).
const FOLD_REACH: f32 = 12.0;
/// The middle of a group's 30px header, where the line turns towards it.
const HEADER_MIDDLE: f32 = 15.0;
/// The turn ends just short of the group's chevron.
const BRANCH_WIDTH: f32 = 6.0;

/// The list's session groups by the project that owns them, for the projects the list draws at
/// the top or inside a project group. Built from the snapshot's groups, which keep a collapsed
/// project's groups so they can fold away with it. gx-core lists an open project's session groups
/// right after it, in `order` or in its project group's ids, and the list skips them there.
pub(crate) struct SessionGroupTree<'a> {
    children: HashMap<&'a str, Vec<&'a NativeSidebarGroup>>,
}

impl<'a> SessionGroupTree<'a> {
    pub(crate) fn new(snapshot: &'a NativeSidebarSnapshot) -> Self {
        let drawn: HashSet<&str> = snapshot
            .order
            .iter()
            .filter(|item| item.kind != "collection")
            .map(|item| item.id.as_str())
            .chain(
                snapshot
                    .collections
                    .iter()
                    .flat_map(|collection| collection.group_ids.iter().map(String::as_str)),
            )
            .collect();
        let mut children: HashMap<&'a str, Vec<&'a NativeSidebarGroup>> = HashMap::new();
        for group in &snapshot.groups {
            if let Some(owner) = group
                .owner_group_id
                .as_deref()
                .filter(|owner| drawn.contains(owner))
            {
                children.entry(owner).or_default().push(group);
            }
        }
        Self { children }
    }

    /// Whether `group` is drawn inside its project's block rather than as a block of its own.
    pub(crate) fn nests(&self, group: &NativeSidebarGroup) -> bool {
        group
            .owner_group_id
            .as_deref()
            .is_some_and(|owner| self.children.contains_key(owner))
    }

    /// The session groups drawn inside the project `group_id`, in list order.
    pub(crate) fn children(&self, group_id: &str) -> &[&'a NativeSidebarGroup] {
        self.children.get(group_id).map_or(&[], Vec::as_slice)
    }
}

impl GhostexGpuiApp {
    /// A project's fold under `key`: its own rows and, when it has session groups, those groups
    /// joined to it by the tree line, so they fold in the project's motion.
    pub(crate) fn render_native_project_fold(
        &self,
        key: String,
        own_rows: AnyElement,
        session_groups: &[&NativeSidebarGroup],
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if session_groups.is_empty() {
            return self.render_native_disclosure(key, own_rows, cx);
        }
        let groups = session_groups
            .iter()
            .map(|group| self.render_native_sidebar_group(group, &[], hud, appearance, cx))
            .collect();
        let tree = project_tree(own_rows, groups, appearance);
        reach_left(self.render_native_disclosure(key, tree, cx), appearance)
    }
}

/// A project's fold content: its own rows, then its session groups, joined by the tree line. The
/// content is padded back by `FOLD_REACH`, which `reach_left` moves the fold left by.
fn project_tree(
    own_rows: AnyElement,
    groups: Vec<AnyElement>,
    appearance: &SidebarAppearance,
) -> AnyElement {
    let scale = appearance.scale;
    let last = groups.len().saturating_sub(1);
    v_flex()
        .w_full()
        .pl(px(FOLD_REACH * scale))
        .child(
            div()
                .relative()
                .w_full()
                .child(own_rows)
                .child(line(appearance).top_0().bottom_0()),
        )
        .children(groups.into_iter().enumerate().map(|(index, group)| {
            div()
                .relative()
                .w_full()
                .pt(px(GROUP_GAP * scale))
                .pl(px(CHILD_INDENT * scale))
                .child(group)
                .child(
                    line(appearance)
                        .top_0()
                        .when(index == last, |line| {
                            line.h(px((GROUP_GAP + HEADER_MIDDLE) * scale))
                        })
                        .when(index != last, |line| line.bottom_0()),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(LINE_X * scale))
                        .top(px((GROUP_GAP + HEADER_MIDDLE - 0.5) * scale))
                        .w(px(BRANCH_WIDTH * scale))
                        .h(px(1.0 * scale))
                        .bg(line_color()),
                )
        }))
        .into_any_element()
}

/// The project's fold, moved left by `FOLD_REACH` so its box takes in the tree line. Its parent
/// stretches it, so it also grows by that much and keeps its right edge.
fn reach_left(fold: AnyElement, appearance: &SidebarAppearance) -> AnyElement {
    div()
        .ml(px(-FOLD_REACH * appearance.scale))
        .child(fold)
        .into_any_element()
}

/// The vertical tree line, positioned by the caller's top and bottom.
fn line(appearance: &SidebarAppearance) -> gpui::Div {
    div()
        .absolute()
        .left(px(LINE_X * appearance.scale))
        .w(px(1.0 * appearance.scale))
        .bg(line_color())
}

/// The colour of a coordinator's tree line (threads.rs), so both trees in the list match.
fn line_color() -> gpui::Hsla {
    chrome_color(0x4a4a4a, 0xc8c8c8).into()
}
