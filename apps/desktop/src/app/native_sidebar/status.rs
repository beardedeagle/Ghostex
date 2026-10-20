use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
    px, rgb,
};

use super::appearance::SidebarAppearance;
use crate::app::helpers::chrome_palette::chrome_color;
use crate::app::helpers::titlebar_background;

/// CDXC:SessionStatus 2026-10-20 DECISION:
/// User: "the pink question dot is smaller than the other dots this is wrong please make all the dots same size (pink one needs to be bigger a bit)". Every status dot in the sidebar (working, attention, question, shell-running, failed) is this one size, so none of them can drift again; the blue attention dot was 7px and the pink question dot 6px before. The project header's count badges (a 6px legend dot beside a number) are not status dots.
pub(crate) const STATUS_DOT_SIZE: f32 = 8.0;

/// CDXC:SessionStatus 2026-09-21 DECISION:
/// User: "I feel we have multiple degrees for the orange color status \"working\" in the sidebar pls unify all of them on this new one you picked", so every working dot, count, and badge in the sidebar uses this one orange (the Spaces badge orange, 20% darker than the old 0xf8ad07 so a white digit stays readable on it).
pub(crate) const WORKING_COLOR: u32 = 0xc68a06;

/// CDXC:SessionStatus 2026-09-19 DECISION:
/// User: "make the working indicator just the orange dot without animation. i dont mind. like the one we have in the SESSIONS header", so a working session shows the same static 8px orange dot the section headers draw.
/// This supersedes the 2026-09-17 decision that brought the animated working spinner back.
/// CDXC:SessionStatus 2026-09-22 DECISION:
/// User: "make shell running just show a #B4B8BF color dot in the sidebar in dark mode and a darker dot in light mode", so a row whose agent still has a background shell or monitor running after its turn draws a grey dot in place of the time; working and attention keep precedence over it. Headers (sections, projects, collections, Spaces, machines) draw the same grey when they have no working session but have a grey-dot session.
pub(crate) fn background_work_color() -> gpui::Rgba {
    chrome_color(0xb4b8bf, 0x6b7078)
}

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: "we must show a red dot on the session in the sidebar if we fail" to apply the model change picked in chat. The red dot wins over working, attention and the grey shell dot, because the messages behind the change are held until the user picks again; the row's tooltip carries the reason.
/// SEE-ALSO: server/src/session_chat_model_selection_alert.rs.
/// CDXC:SessionStatus 2026-10-14 WHY: a session whose program reported `error` over OSC 7501 (while those reports drive its status) draws the same red dot until the user has seen it; the row's tooltip carries the program's message.
pub(crate) const FAILED_COLOR: u32 = 0xef4444;

/// The pink dot of a session with an unanswered question card.
pub(crate) const QUESTION_COLOR: u32 = 0xf472b6;

pub(crate) fn activity_indicator(
    activity: &str,
    has_background_work: bool,
    model_selection_failed: bool,
    scale: f32,
) -> Option<AnyElement> {
    let indicator = match activity {
        _ if model_selection_failed => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(rgb(FAILED_COLOR))
            .into_any_element(),
        "working" => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(rgb(WORKING_COLOR))
            .into_any_element(),
        "attention" => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(rgb(0x95d7f6))
            .into_any_element(),
        _ if has_background_work => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(background_work_color())
            .into_any_element(),
        _ => return None,
    };
    Some(
        div()
            .w(px(19.0 * scale))
            .h(px(16.0 * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(indicator)
            .into_any_element(),
    )
}

/// What a [`status_dot_stack`] sits on, so each dot's ring matches it: the colour at rest, the colour while the pointer is over the owner, and the owner's GPUI group name (the owner calls `.group(..)` with the same name).
pub(crate) struct DotBackdrop {
    rest: Hsla,
    hover: Hsla,
    pub(crate) group: SharedString,
}

impl DotBackdrop {
    /// `wash` is the owner's own fill over the sidebar (a selected Space's, say), `None` for the bare sidebar.
    pub(crate) fn new(
        appearance: &SidebarAppearance,
        wash: Option<Hsla>,
        group: impl Into<SharedString>,
    ) -> Self {
        let base = titlebar_background();
        Self {
            rest: wash.map_or(base, |wash| base.blend(wash)),
            hover: base.blend(appearance.hover),
            group: group.into(),
        }
    }
}

/// CDXC:Spaces 2026-10-20 DECISION:
/// User: the status dots under a Space, and on a remote machine tab, overlap by half instead of sitting side by side: blue most right and on top, orange in the middle, the grey shell-running dot (which never shows with orange) behind them. The pink question dot is shown there too, "as last one from the left covered by the orange status dot", so a session with an unanswered question counts pink instead of blue, as its session card shows it. Every dot is `STATUS_DOT_SIZE`, and "when we show the dots on top of each other we need to have 1px border around the dots": each dot has a 1px ring in the colour of what is behind the stack (the sidebar, a selected Space's fill, the hover fill), outside its 8px. This supersedes the 2026-09-26 "no outline on these indicator dots please at all" and its mixed 8/7/6px sizes.
/// CDXC:Spaces 2026-09-26 WHY:
/// The stack has an explicit width with each dot placed at a fixed offset instead of negative flex margins, which left the measured box narrower than the drawn dots and pushed two or more of them right of centre.
pub(crate) fn status_dot_stack(
    working_count: usize,
    attention_count: usize,
    background_work_count: usize,
    question_count: usize,
    scale: f32,
    backdrop: DotBackdrop,
) -> gpui::Div {
    let background = working_count == 0 && background_work_count > 0;
    let dots: Vec<Hsla> = [
        (question_count > 0, rgb(QUESTION_COLOR).into()),
        (background, background_work_color().into()),
        (working_count > 0, rgb(WORKING_COLOR).into()),
        (attention_count > 0, rgb(0x95d7f6).into()),
    ]
    .into_iter()
    .filter(|(shown, _)| *shown)
    .map(|(_, color)| color)
    .collect();
    let ring = 1.0 * scale;
    let size = STATUS_DOT_SIZE * scale + 2.0 * ring;
    let step = 4.0 * scale;
    let width = dots.len().saturating_sub(1) as f32 * step + size;
    let hover = backdrop.hover;
    let group = backdrop.group;
    div()
        .relative()
        .flex_shrink_0()
        .w(px(width))
        .h(px(size))
        .children(dots.into_iter().enumerate().map(|(index, color)| {
            div()
                .absolute()
                .left(px(index as f32 * step))
                .top_0()
                .size(px(size))
                .rounded_full()
                .border(px(ring))
                .border_color(backdrop.rest)
                .group_hover(group.clone(), move |dot| dot.border_color(hover))
                .bg(color)
        }))
}

pub(crate) fn question_indicator(working: bool, scale: f32) -> AnyElement {
    div()
        .h(px(16.0 * scale))
        .min_w(px(16.0 * scale))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(4.0 * scale))
        .when(working, |indicator| {
            indicator.child(
                div()
                    .size(px(STATUS_DOT_SIZE * scale))
                    .rounded_full()
                    .bg(rgb(WORKING_COLOR)),
            )
        })
        .child(
            div()
                .size(px(STATUS_DOT_SIZE * scale))
                .rounded_full()
                .bg(rgb(QUESTION_COLOR)),
        )
        .into_any_element()
}

pub(crate) fn completion_opacity(start: web_time::Instant) -> f32 {
    let progress = (start.elapsed().as_secs_f32() / 3.0).min(1.0);
    let stops = [
        (0.0, 1.0),
        (0.08, 0.9),
        (0.16, 0.58),
        (0.24, 1.0),
        (0.36, 0.9),
        (0.44, 0.58),
        (0.52, 1.0),
        (0.64, 0.9),
        (0.72, 0.58),
        (0.80, 1.0),
        (1.0, 1.0),
    ];
    for pair in stops.windows(2) {
        if progress <= pair[1].0 {
            let t = (progress - pair[0].0) / (pair[1].0 - pair[0].0);
            return pair[0].1 + (pair[1].1 - pair[0].1) * t;
        }
    }
    1.0
}

/// CDXC:SessionStatus 2026-09-28 DECISION:
/// User chose the soft fill, only for confirmed human input, as an Advanced setting off by default. Pending questions remain highlighted while working; generic attention is not evidence of a question. Design evidence: branch prototype/sidebar-attention-2026-09-28.
pub(crate) fn pending_question_fill(
    settings: &serde_json::Value,
    pending_question: bool,
) -> Option<gpui::Rgba> {
    (pending_question && settings["highlightPendingQuestions"].as_bool() == Some(true))
        .then(|| rgb(0xf472b6).opacity(0.12))
}
