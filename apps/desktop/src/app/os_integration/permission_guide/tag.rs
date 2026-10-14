//! The edge tag's drawing: a Ghostex tag on System Settings' right edge, its notch pointing into
//! the app list, two progress segments along its top, the spaceo tile to drag into the list with
//! orange chevrons running toward it, and the words, status and buttons of each step. Only GPUI,
//! so native-modal-demo draws it too.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt as _, AnyElement, BoxShadow, Context, Hsla, MouseButton,
    MouseDownEvent, Rgba, SharedString, Window, div, img, linear_color_stop, linear_gradient,
    point, px, relative, rgb, rgba, svg,
};

pub(crate) const TAG_WIDTH: f32 = 286.0;
/// Room left of the tag for its notch.
pub(crate) const NOTCH_ROOM: f32 = 10.0;
pub(crate) const WINDOW_WIDTH: f32 = TAG_WIDTH + NOTCH_ROOM;
/// How far below the window's top the notch's tip points.
pub(crate) const NOTCH_Y: f32 = 31.0;
/// How long All set shows before the tag tucks away.
pub(crate) const CLOSING: Duration = Duration::from_secs(3);

const PANEL: u32 = 0x161616;
const INK: u32 = 0xe8eaee;
const TEXT: u32 = 0xc8cdd5;
const MUTED: u32 = 0x8a9099;
const ORANGE: u32 = 0xf5a524;
const GREEN: u32 = 0x4ade80;

const PAD: f32 = 12.0;
const HERO: f32 = 58.0;
const GAP: f32 = 11.0;
const TITLE_LINE: f32 = 18.0;
const SUB_LINE: f32 = 16.0;
const DONE_LINE: f32 = 21.0;
const FOOT: f32 = 26.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagSegment {
    Next,
    Wait,
    Ok,
    Restart,
}

/// Which side of System Settings the tag sits on: the right, or the left when the right has no
/// room on screen. The notch, chevrons and lean all point at the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagSide {
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagFoot {
    /// "Waiting for the drop" and "1 of 2".
    Waiting {
        step: usize,
    },
    Granted,
    /// "Restarting", or "Waiting for agents" while an agent is using SpaceO.
    Restarting {
        busy: bool,
    },
    /// A draining line, then the tag tucks away.
    Closing,
}

pub(crate) struct TagContent {
    pub(crate) title: String,
    pub(crate) subtitle: String,
    /// Lines kept for the subtitle; every step's fits on one.
    pub(crate) subtitle_lines: usize,
    /// "Screen Recording granted" under the title once one permission is in.
    pub(crate) done_line: Option<String>,
    /// The spaceo tile, while there is something to drop.
    pub(crate) tile: bool,
    /// The file name on the tile.
    pub(crate) file_name: String,
    pub(crate) foot: TagFoot,
    pub(crate) segments: [TagSegment; 2],
    pub(crate) side: TagSide,
}

/// The tag's height for `content`; every line has a fixed height, so the window fits it exactly.
pub(crate) fn tag_height(content: &TagContent) -> f32 {
    let mut height =
        PAD + TITLE_LINE + 2.0 + SUB_LINE * content.subtitle_lines as f32 + GAP + FOOT + PAD;
    if content.tile {
        height += HERO + GAP;
    }
    if content.done_line.is_some() {
        height += DONE_LINE;
    }
    height
}

fn ink(color: u32) -> Hsla {
    rgb(color).into()
}

fn alpha(color: u32, opacity: f32) -> Hsla {
    let mut color: Hsla = rgb(color).into();
    color.a = opacity;
    color
}

fn segment_color(segment: TagSegment) -> Rgba {
    match segment {
        TagSegment::Next => rgba(0xffffff1f),
        TagSegment::Wait | TagSegment::Restart => rgb(ORANGE),
        TagSegment::Ok => rgb(GREEN),
    }
}

/// A dot and a word: orange and breathing while waiting, green once granted.
fn status(label: &'static str, color: u32, breathing: bool) -> AnyElement {
    // While waiting, a round ring grows out of the dot and fades, every 1.8s.
    let ring = breathing.then(|| {
        div().absolute().rounded_full().with_animation(
            "tag-status-breathe",
            Animation::new(Duration::from_millis(1800)).repeat(),
            move |ring, delta| {
                let grow = if delta < 0.6 { delta / 0.6 } else { 0.0 };
                let spread = 5.0 * grow;
                let fade = if delta < 0.6 {
                    0.55 * (1.0 - grow)
                } else {
                    0.0
                };
                ring.left(px(-spread))
                    .top(px(-spread))
                    .size(px(7.0 + 2.0 * spread))
                    .bg(alpha(color, fade))
            },
        )
    });
    let dot = div()
        .relative()
        .flex_none()
        .size(px(7.0))
        .children(ring)
        .child(div().absolute().size_full().rounded_full().bg(ink(color)))
        .into_any_element();
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(ink(color))
        .child(dot)
        .child(label)
        .into_any_element()
}

/// Three chevrons pointing at the list, lighting up one after another toward it.
fn trail(side: TagSide) -> AnyElement {
    let (path, lags) = match side {
        TagSide::Right => ("modals/permission-guide/chevron-left.svg", [0.3, 0.15, 0.0]),
        TagSide::Left => (
            "modals/permission-guide/chevron-right.svg",
            [0.0, 0.15, 0.3],
        ),
    };
    div()
        .flex()
        .flex_none()
        .children((0..3).map(|index| {
            // The chevron nearest the list lights first.
            let lag = lags[index] / 1.5;
            svg()
                .path(path)
                .w(px(9.0))
                .h(px(14.0))
                .text_color(ink(ORANGE))
                .with_animation(
                    SharedString::from(format!("tag-trail-{index}")),
                    Animation::new(Duration::from_millis(1500)).repeat(),
                    move |chevron, delta| {
                        let t = (delta - lag).rem_euclid(1.0);
                        let wave = if t < 0.4 {
                            t / 0.4
                        } else {
                            1.0 - (t - 0.4) / 0.6
                        };
                        chevron.opacity(0.2 + 0.8 * wave)
                    },
                )
        }))
        .into_any_element()
}

/// The spaceo file as its own small icon: black, with "exec" in green.
fn exec_icon() -> AnyElement {
    div()
        .relative()
        .flex_none()
        .size(px(36.0))
        .rounded(px(6.0))
        .bg(rgb(0x050505))
        .border_1()
        .border_color(rgb(0x4a4a4f))
        .child(
            div()
                .absolute()
                .left(px(4.0))
                .top(px(3.0))
                .font_family("Menlo")
                .text_size(px(9.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0x3ae35a))
                .child("exec"),
        )
        .into_any_element()
}

fn grip() -> AnyElement {
    div()
        .flex_none()
        .w(px(8.0))
        .h(px(16.0))
        .flex()
        .flex_wrap()
        .gap(px(2.0))
        .opacity(0.5)
        .children((0..8).map(|_| div().size(px(2.0)).rounded_full().bg(rgb(0xffffff))))
        .into_any_element()
}

pub(crate) struct TagActions<V: 'static> {
    pub(crate) drag: Box<dyn Fn(&mut V, &mut Window, &mut Context<V>)>,
    pub(crate) close: Box<dyn Fn(&mut V, &mut Window, &mut Context<V>)>,
}

pub(crate) fn render_tag<V: 'static>(
    content: TagContent,
    actions: TagActions<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let TagActions { drag, close } = actions;
    let side = content.side;
    let progress = div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .px(px(PAD))
        .flex()
        .gap(px(3.0))
        .children(content.segments.map(|segment| {
            div()
                .flex_1()
                .h(px(2.0))
                .rounded_b(px(2.0))
                .bg(segment_color(segment))
        }));
    let tile = content.tile.then(|| {
        div()
            .id("permission-guide-tile")
            .h(px(HERO))
            .mb(px(GAP))
            .pl(px(11.0))
            .pr(px(12.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .rounded(px(11.0))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(rgb(0x2a2a2a), 0.0),
                linear_color_stop(rgb(0x222222), 1.0),
            ))
            .border_1()
            .border_color(rgba(0xffffff1c))
            .shadow(vec![BoxShadow {
                color: gpui::hsla(0.0, 0.0, 0.0, 0.45),
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(18.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .cursor_grab()
            .text_color(rgb(INK))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| drag(view, window, cx)),
            )
            .when(side == TagSide::Right, |tile| tile.child(trail(side)))
            .child(exec_icon())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(14.5))
                            .line_height(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(content.file_name.clone()),
                    )
                    .child(
                        div()
                            .mt(px(1.0))
                            .text_size(px(11.5))
                            .line_height(px(14.0))
                            .text_color(rgb(MUTED))
                            .child("Drop it on the list"),
                    ),
            )
            .child(grip())
            .when(side == TagSide::Left, |tile| tile.child(trail(side)))
            // Every 2.6s the tile leans toward the list, then settles.
            .with_animation(
                "permission-guide-lean",
                Animation::new(Duration::from_millis(2600)).repeat(),
                move |tile, delta| {
                    let shift = if delta < 0.7 {
                        0.0
                    } else if delta < 0.8 {
                        -5.0 * (delta - 0.7) / 0.1
                    } else if delta < 0.9 {
                        -5.0 + 6.0 * (delta - 0.8) / 0.1
                    } else {
                        1.0 - (delta - 0.9) / 0.1
                    };
                    let toward = if side == TagSide::Right { 1.0 } else { -1.0 };
                    tile.relative().left(px(shift * toward))
                },
            )
    });
    let top = div()
        .flex()
        .items_start()
        .gap(px(9.0))
        .pl(px(2.0))
        .pt(px(2.0))
        .child(
            img("app-icon.png")
                .mt(px(2.0))
                .size(px(16.0))
                .rounded(px(4.0))
                .flex_none(),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(TITLE_LINE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(INK))
                        .child(content.title),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .line_height(px(SUB_LINE))
                        .text_color(rgb(MUTED))
                        .child(content.subtitle),
                )
                .children(content.done_line.map(|line| {
                    div()
                        .mt(px(5.0))
                        .h(px(DONE_LINE - 5.0))
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .text_size(px(12.0))
                        .text_color(rgb(GREEN))
                        .child(
                            svg()
                                .path("modals/permission-guide/check.svg")
                                .size(px(12.0))
                                .text_color(rgb(GREEN)),
                        )
                        .child(line)
                })),
        )
        .child(
            div()
                .id("permission-guide-close")
                .flex_none()
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .text_color(rgb(MUTED))
                .hover(|style| style.bg(rgba(0xffffff1a)).text_color(rgb(INK)))
                .cursor_pointer()
                .on_click(cx.listener(move |view, _, window, cx| close(view, window, cx)))
                .child(
                    svg()
                        .path("modals/permission-guide/close.svg")
                        .size(px(11.0))
                        .text_color(rgb(MUTED)),
                ),
        );
    let foot = div()
        .mt(px(GAP))
        .h(px(FOOT))
        .px(px(2.0))
        .flex()
        .items_center()
        .gap(px(8.0));
    let foot = match content.foot {
        TagFoot::Waiting { step } => foot
            .child(status("Waiting for the drop", ORANGE, true))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(MUTED))
                    .child(format!("{step} of 2")),
            ),
        TagFoot::Granted => foot.child(status("Granted", GREEN, false)),
        TagFoot::Restarting { busy } => foot.child(status(
            if busy {
                "Waiting for agents"
            } else {
                "Restarting"
            },
            ORANGE,
            true,
        )),
        TagFoot::Closing => foot
            .child(
                div()
                    .flex_1()
                    .h(px(2.0))
                    .rounded(px(2.0))
                    .bg(rgba(0xffffff14))
                    .overflow_hidden()
                    .child(div().h_full().bg(rgb(GREEN)).with_animation(
                        "permission-guide-closing",
                        Animation::new(CLOSING),
                        |line, delta| line.w(relative(1.0 - delta)),
                    )),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(MUTED))
                    .child("Closing"),
            ),
    };
    let (notch_path, notch_left, edge_shift, panel_left) = match side {
        TagSide::Right => ("modals/permission-guide/notch.svg", 0.0, 1.0, NOTCH_ROOM),
        TagSide::Left => (
            "modals/permission-guide/notch-right.svg",
            TAG_WIDTH - 1.0,
            -1.0,
            0.0,
        ),
    };
    let notch = div()
        .absolute()
        .left(px(notch_left))
        .top(px(NOTCH_Y - 10.0))
        .w(px(NOTCH_ROOM + 1.0))
        .h(px(20.0))
        .child(
            svg()
                .absolute()
                .left(px(if edge_shift > 0.0 { 0.0 } else { 1.0 }))
                .top_0()
                .w(px(NOTCH_ROOM))
                .h(px(20.0))
                .path(notch_path)
                .text_color(rgba(0xffffff21)),
        )
        .child(
            svg()
                .absolute()
                .left(px(if edge_shift > 0.0 { 1.0 } else { 0.0 }))
                .top_0()
                .w(px(NOTCH_ROOM))
                .h(px(20.0))
                .path(notch_path)
                .text_color(rgb(PANEL)),
        );
    div()
        .size_full()
        .relative()
        .font_family(crate::ui_fonts::UI_FONT)
        .text_color(rgb(TEXT))
        .child(
            div()
                .absolute()
                .left(px(panel_left))
                .top_0()
                .w(px(TAG_WIDTH))
                .h_full()
                .rounded(px(12.0))
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgba(0xffffff21))
                .child(progress)
                .child(div().p(px(PAD - 1.0)).children(tile).child(top).child(foot)),
        )
        .child(notch)
        .into_any_element()
}

/// Which of the two permissions the tag is on; Screen Recording comes first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagPermission {
    ScreenRecording,
    Accessibility,
}

impl TagPermission {
    pub(crate) fn other(self) -> Self {
        match self {
            Self::ScreenRecording => Self::Accessibility,
            Self::Accessibility => Self::ScreenRecording,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ScreenRecording => "Screen Recording",
            Self::Accessibility => "Accessibility",
        }
    }

    /// The list's heading in System Settings.
    fn pane(self) -> &'static str {
        match self {
            Self::ScreenRecording => "Screen & System Audio Recording",
            Self::Accessibility => "Accessibility",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagStage {
    /// Waiting for spaceo to be dropped into the current list.
    Waiting,
    /// The current permission just landed; the next pane opens.
    Dropped,
    /// Both are in, but SpaceO started before one of them: it restarts by itself, after the
    /// `agents` using it finish.
    Restarting { agents: usize },
    /// Both work; the tag closes after a short countdown.
    Ready,
}

/// What macOS has for one permission: in the list and on, and whether SpaceO still needs a
/// restart to use it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TagGrant {
    pub(crate) has: bool,
    pub(crate) needs_restart: bool,
}

/// The words, tile, footer and progress for a step, shared by the app and native-modal-demo.
pub(crate) fn tag_content(
    stage: TagStage,
    current: TagPermission,
    screen_recording: TagGrant,
    accessibility: TagGrant,
    file_name: &str,
    side: TagSide,
) -> TagContent {
    let grant = |permission: TagPermission| match permission {
        TagPermission::ScreenRecording => screen_recording,
        TagPermission::Accessibility => accessibility,
    };
    let restarting = matches!(stage, TagStage::Restarting { .. });
    let segments =
        [TagPermission::ScreenRecording, TagPermission::Accessibility].map(|permission| {
            let grant = grant(permission);
            if restarting && grant.needs_restart {
                TagSegment::Restart
            } else if grant.has || (permission == current && stage != TagStage::Waiting) {
                TagSegment::Ok
            } else if permission == current {
                TagSegment::Wait
            } else {
                TagSegment::Next
            }
        });
    let other = current.other();
    let (title, subtitle, subtitle_lines, done_line, tile, foot) = match stage {
        TagStage::Waiting => {
            let second = grant(other).has;
            (
                if second {
                    "Drag SpaceO into this list too"
                } else {
                    "Drag SpaceO into the list"
                }
                .to_string(),
                current.pane().to_string(),
                1,
                second.then(|| format!("{} granted", other.name())),
                true,
                TagFoot::Waiting {
                    step: if second { 2 } else { 1 },
                },
            )
        }
        TagStage::Dropped => (
            "SpaceO is in and switched on".to_string(),
            format!("Opening {} next", other.name()),
            1,
            None,
            false,
            TagFoot::Granted,
        ),
        TagStage::Restarting { agents: 0 } => (
            "Restarting SpaceO".to_string(),
            "So it can use the new permissions.".to_string(),
            1,
            None,
            false,
            TagFoot::Restarting { busy: false },
        ),
        TagStage::Restarting { agents } => (
            "Restarts when SpaceO is free".to_string(),
            if agents == 1 {
                "An agent is using it right now.".to_string()
            } else {
                format!("{agents} agents are using it right now.")
            },
            1,
            None,
            false,
            TagFoot::Restarting { busy: true },
        ),
        TagStage::Ready => (
            "SpaceO is ready".to_string(),
            "Both permissions granted".to_string(),
            1,
            None,
            false,
            TagFoot::Closing,
        ),
    };
    TagContent {
        title,
        subtitle,
        subtitle_lines,
        done_line,
        tile,
        file_name: file_name.to_string(),
        foot,
        segments,
        side,
    }
}
