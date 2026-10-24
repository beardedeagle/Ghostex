//! A coordinator's tree in the sidebar: indented thread rows with a tree line, the icon and badge on
//! the coordinator row, and its "N older threads" row. Visual only; the row itself stays the click,
//! drag and menu target.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/threads.rs (the order, depth and which threads are
//! older), apps/desktop/src/app/gx_store/sidebar_snapshot.rs (`threadDepth`, `threadLast`,
//! `coordinatorThreads`).

use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px, rgb,
};
use gpui_component::v_flex;
use serde_json::{Value, json};

use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarGroup, NativeSidebarSession},
};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// Indent per tree level; one agent icon plus the row gap, so a thread's icon sits under its
/// coordinator's title.
pub(crate) const THREAD_INDENT: f32 = 16.0;
/// The coordinator marker and its thread count.
const COORDINATOR_ICON: &str = "titlebar/users-group.svg";
const WAITING_COLOR: u32 = 0x95d7f6;
const COORDINATOR_ROW_ICON: &str = "titlebar/coordinator-crown.svg";
const COORDINATOR_COLOR_DARK: u32 = 0xffffff;
const COORDINATOR_COLOR_LIGHT: u32 = 0x000000;

pub(crate) fn thread_depth(session: &NativeSidebarSession) -> f32 {
    session
        .details
        .get("threadDepth")
        .and_then(Value::as_u64)
        .unwrap_or(0) as f32
}

/// The tree line: from the top of the row down to the icon (the last thread) or through the row
/// (a thread with siblings below it), then across to the icon. `card_height` is this row's card and
/// `above_height` the card above it, both unscaled: a work-mode card is taller, but its icon still
/// sits centred in its first `SESSION_HEIGHT`.
pub(crate) fn thread_connector(
    session: &NativeSidebarSession,
    card_height: f32,
    above_height: f32,
    appearance: &SidebarAppearance,
) -> Option<AnyElement> {
    let depth = thread_depth(session);
    if depth < 1.0 {
        return None;
    }
    let scale = appearance.scale;
    let last = session.details.get("threadLast").and_then(Value::as_bool) == Some(true);
    let color = chrome_color(0x4a4a4a, 0xc8c8c8);
    // Under the middle of the parent's 15px icon, which starts at the row's 5px inset.
    let x = (5.0 + 7.0 + (depth - 1.0) * THREAD_INDENT) * scale;
    let height = card_height * scale;
    let mid = super::session_list::SESSION_HEIGHT * scale / 2.0;
    // The line starts right under the icon of the row above, centred in that row's first line.
    let rise =
        (above_height * scale - mid - 7.5 * scale) + super::session_list::SESSION_SPACING * scale;
    Some(
        div()
            .absolute()
            .top(px(-rise))
            .left(px(x))
            .w(px((THREAD_INDENT - 5.0) * scale))
            .h(if last {
                px(rise + mid)
            } else {
                px(rise + height + super::session_list::SESSION_SPACING * scale)
            })
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(1.0 * scale))
                    .bg(color),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(rise + mid - 0.5 * scale))
                    .w(px((THREAD_INDENT - 6.0) * scale))
                    .h(px(1.0 * scale))
                    .bg(color),
            )
            .into_any_element(),
    )
}

pub(crate) fn is_coordinator(session: &NativeSidebarSession) -> bool {
    session
        .details
        .get("isCoordinator")
        .and_then(Value::as_bool)
        == Some(true)
}

/// The icon a coordinator row draws in place of its agent's logo.
///
/// CDXC:Coordinators 2026-10-01 DECISION:
/// User: "please give coordinator agents a different logo in the sidebar of the app (not the agent's app logo)", a cool SVG instead of the Claude icon. A crown, bold enough to read at the row's 13px, drawn white on dark themes and black on light ones the way the Codex logo adapts (user: "make the crown white, not purple"), with the row's usual focus and hover dimming; its threads keep their agent logos, and the crew icon with the thread count stays beside it.
pub(crate) fn coordinator_icon(appearance: &SidebarAppearance) -> AnyElement {
    titlebar_svg_icon(
        COORDINATOR_ROW_ICON,
        13.0 * appearance.scale,
        rgb(if appearance.light {
            COORDINATOR_COLOR_LIGHT
        } else {
            COORDINATOR_COLOR_DARK
        })
        .into(),
    )
    .into_any_element()
}

/// The size of the row's time text in `sessions.rs`; the badge's icon and count use it so they sit on the time's baseline.
const BADGE_TEXT_SIZE: f32 = 13.55;

/// The coordinator row's marker: the crew icon and its thread count, tinted when a thread waits on
/// someone (light blue) or works (orange).
///
/// CDXC:Coordinators 2026-09-30 WHY:
/// A coordinator looks like any other session of its agent otherwise, and its thread rows alone do not say which row they hang from once the list scrolls (or once the tree is folded). What the numbers mean is gx-core's `RowNesting::coordinator_badge`.
pub(crate) fn coordinator_badge(
    session: &NativeSidebarSession,
    appearance: &SidebarAppearance,
) -> Option<AnyElement> {
    if !is_coordinator(session) {
        return None;
    }
    let scale = appearance.scale;
    let threads = session.details.get("coordinatorThreads");
    let count = threads
        .and_then(|threads| threads.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let tint = match threads
        .and_then(|threads| threads.get("tone"))
        .and_then(Value::as_str)
    {
        Some("waiting") => rgb(WAITING_COLOR).into(),
        Some("working") => rgb(super::status::WORKING_COLOR).into(),
        _ => appearance.muted,
    };
    Some(
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(2.0 * scale))
            .child(titlebar_svg_icon(
                COORDINATOR_ICON,
                BADGE_TEXT_SIZE * scale,
                tint,
            ))
            .when(count > 0, |badge| {
                badge.child(
                    div()
                        .text_size(px(BADGE_TEXT_SIZE * scale))
                        .text_color(tint)
                        .child(count.to_string()),
                )
            })
            .into_any_element(),
    )
}

/// An expanded coordinator whose older threads wait behind its "N older threads" row.
struct OlderThreadsRow {
    coordinator: String,
    depth: f32,
    count: u64,
    shown: bool,
}

fn older_threads_row(session: &NativeSidebarSession) -> Option<OlderThreadsRow> {
    let threads = session.details.get("coordinatorThreads")?;
    let count = threads.get("older").and_then(Value::as_u64).unwrap_or(0);
    if count == 0 || threads.get("collapsed").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(OlderThreadsRow {
        coordinator: session.session_id.clone(),
        depth: thread_depth(session),
        count,
        shown: threads.get("olderShown").and_then(Value::as_bool) == Some(true),
    })
}

impl GhostexGpuiApp {
    /// A heading's rows, with each expanded coordinator's "N older threads" row after the last row
    /// of its tree. The rows between two of those stay one virtualized list.
    pub(super) fn render_native_session_rows(
        &self,
        group: &NativeSidebarGroup,
        sessions: Vec<Arc<NativeSidebarSession>>,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if !sessions
            .iter()
            .any(|session| older_threads_row(session).is_some())
        {
            return self.render_native_session_list(group, sessions, appearance, cx);
        }
        let mut parts = Vec::new();
        let mut segment = Vec::new();
        let mut open: Vec<OlderThreadsRow> = Vec::new();
        let close = |row: OlderThreadsRow,
                     segment: &mut Vec<Arc<NativeSidebarSession>>,
                     parts: &mut Vec<AnyElement>,
                     cx: &mut gpui::Context<Self>| {
            if !segment.is_empty() {
                parts.push(self.render_native_session_list(
                    group,
                    std::mem::take(segment),
                    appearance,
                    cx,
                ));
            }
            parts.push(self.render_older_threads_row(row, appearance, cx));
        };
        for session in sessions {
            let depth = thread_depth(&session);
            while open.last().is_some_and(|row| row.depth >= depth) {
                let row = open.pop().expect("checked above");
                close(row, &mut segment, &mut parts, cx);
            }
            if let Some(row) = older_threads_row(&session) {
                open.push(row);
            }
            segment.push(session);
        }
        while let Some(row) = open.pop() {
            close(row, &mut segment, &mut parts, cx);
        }
        if !segment.is_empty() {
            parts.push(self.render_native_session_list(group, segment, appearance, cx));
        }
        v_flex()
            .w_full()
            .flex_shrink_0()
            .children(parts)
            .into_any_element()
    }

    /// The quiet row that lists a coordinator's older threads, or tucks them away again.
    fn render_older_threads_row(
        &self,
        row: OlderThreadsRow,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let label = match (row.shown, row.count) {
            (true, _) => "Hide older threads".to_string(),
            (false, 1) => "1 older thread".to_string(),
            (false, count) => format!("{count} older threads"),
        };
        let coordinator = row.coordinator.clone();
        div()
            .id(gpui::SharedString::from(format!(
                "native-sidebar-older-threads-{}",
                row.coordinator
            )))
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .mx(px(super::session_list::SESSION_INSET_X * scale))
            .mb(px(super::session_list::SESSION_SPACING * scale))
            .h(px(24.0 * scale))
            // Lines up with the thread titles: a thread row's inset, its 15px icon and the 6px gap
            // after it (`render_native_sidebar_session`).
            .pl(px(
                (5.0 + (row.depth + 1.0) * THREAD_INDENT + 15.0 + 6.0) * scale
            ))
            .flex()
            .items_center()
            .rounded(px(5.0 * scale))
            .text_size(px(11.5 * scale))
            .text_color(appearance.muted)
            .cursor_pointer()
            .hover(|style| style.bg(appearance.session_hover))
            .child(label)
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "toggleCoordinatorOlder", "sessionId": coordinator}),
                    cx,
                );
            }))
            .into_any_element()
    }
}

/// The overlapping dots a folded coordinator draws for the threads its fold hides, with its own
/// status (gx-core `RowNesting::folded_status`); `None` while its threads are open or none has a dot.
///
/// CDXC:Coordinators 2026-10-24 SEE-ALSO: the user's decision on gx-core `RowNesting::folded_status` ("show 2 dots (yellow and blue) in this case (they need to be overlapping dots pls)"); the phone draws the same stack in apps/mobile/app/src/components/sessions/SessionRow.tsx.
pub(crate) fn folded_status_indicator(
    session: &NativeSidebarSession,
    scale: f32,
    ring: gpui::Hsla,
) -> Option<AnyElement> {
    let status = session
        .details
        .get("coordinatorThreads")?
        .get("foldedStatus")?;
    let flag = |name: &str| status.get(name).and_then(Value::as_bool) == Some(true);
    Some(super::status::folded_status_indicator(
        flag("working"),
        flag("attention"),
        flag("backgroundWork"),
        flag("question"),
        scale,
        ring,
    ))
}

/// Whether a coordinator's threads can be folded, and if so whether they are folded now.
pub(crate) fn coordinator_fold_state(session: &NativeSidebarSession) -> Option<bool> {
    let threads = session.details.get("coordinatorThreads")?;
    if threads.get("collapsible").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(threads.get("collapsed").and_then(Value::as_bool) == Some(true))
}

/// A folded coordinator that stands in for the focused thread it hides (gx-core
/// `RowNesting::contains_active_session`).
pub(crate) fn coordinator_holds_active(session: &NativeSidebarSession) -> bool {
    session
        .details
        .get("coordinatorThreads")
        .and_then(|threads| threads.get("containsActive"))
        .and_then(Value::as_bool)
        == Some(true)
}

/// The hollow circle a collapsed section heading draws when the focused session is inside it; a
/// coordinator draws it in its crown's spot for a hidden focused thread.
pub(crate) fn contains_active_marker(appearance: &SidebarAppearance) -> AnyElement {
    let scale = appearance.scale;
    div()
        .size(px(9.0 * scale))
        .rounded_full()
        .border(px(1.5 * scale))
        .border_color(chrome_color(0xa3a3a3, 0x858585))
        .into_any_element()
}

/// The fold chevron a hovered coordinator card draws in the crown's spot: pointing right while the
/// threads are folded, down while they are open, like a project's.
///
/// CDXC:Coordinators 2026-10-06 DECISION:
/// User: "Please don't show the chevron here separately, this chevron needs to be shown instead of the coordinator icon when I hover on the coordinator card in the sidebar." The card has no chevron slot of its own; the crown's spot is the one hit area (`render_native_session_identity`) and shows this chevron while the card is hovered. This supersedes the separate chevron beside the crown that `render_coordinator_chevron` drew from 2026-09-30.
pub(crate) fn coordinator_fold_chevron(
    collapsed: bool,
    appearance: &SidebarAppearance,
) -> AnyElement {
    gpui::svg()
        .path(crate::app::consts::COMMAND_ICON_CHEVRON_RIGHT)
        .size(px(14.0 * appearance.scale))
        .text_color(appearance.muted)
        .with_transformation(gpui::Transformation::rotate(gpui::percentage(
            if collapsed { 0.0 } else { 0.25 },
        )))
        .into_any_element()
}
