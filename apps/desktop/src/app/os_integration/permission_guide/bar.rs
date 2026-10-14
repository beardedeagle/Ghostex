//! The floating permission bar's drawing: the draggable file tile, what to do now, the live status
//! with the two-step progress, and the close button. Only GPUI, so native-modal-demo shows it too.

use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, MouseDownEvent, Window, div, px, rgb, rgba, svg};

pub(crate) const BAR_WIDTH: f32 = 560.0;
pub(crate) const BAR_HEIGHT: f32 = 68.0;

const PANEL: u32 = 0x161616;
const CARD: u32 = 0x1d1d1d;
const INK: u32 = 0xededed;
const DIM: u32 = 0x9a9a9a;
const WAIT: u32 = 0xfbbf24;
const OK: u32 = 0x34d399;
const TODO: u32 = 0x4a4a4a;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarStep {
    Done,
    Current,
    Todo,
}

pub(crate) struct BarContent {
    pub(crate) title: String,
    pub(crate) detail: String,
    /// The file the tile drags, as macOS lists it (`spaceo`).
    pub(crate) file_name: String,
    /// The current permission has landed: the bar turns green.
    pub(crate) landed: bool,
    /// "1 of 2"; `None` once everything is done.
    pub(crate) step: Option<usize>,
    pub(crate) steps: [BarStep; 2],
}

pub(crate) fn render_bar<V: 'static>(
    content: BarContent,
    on_drag: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let accent = if content.landed { OK } else { WAIT };
    let tile = div()
        .id("permission-guide-tile")
        .flex_shrink_0()
        .h(px(46.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .rounded(px(10.0))
        .bg(rgb(CARD))
        .border_1()
        .border_dashed()
        .border_color(rgba((accent << 8) | 0x99))
        .cursor_grab()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, window, cx| on_drag(view, window, cx)),
        )
        .child(
            svg()
                .path("modals/settings/terminal-2.svg")
                .size(px(18.0))
                .text_color(rgb(INK)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgb(INK))
                        .child(content.file_name),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(DIM))
                        .child("drag into the list"),
                ),
        );
    let status = div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(div().size(px(6.0)).rounded_full().bg(rgb(accent)))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(accent))
                        .child(if content.landed { "Granted" } else { "Waiting" }),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .children(content.steps.map(|step| {
                    let color = match step {
                        BarStep::Done => OK,
                        BarStep::Current => WAIT,
                        BarStep::Todo => TODO,
                    };
                    div().w(px(14.0)).h(px(3.0)).rounded(px(2.0)).bg(rgb(color))
                }))
                .children(content.step.map(|step| {
                    div()
                        .ml(px(4.0))
                        .text_size(px(10.5))
                        .text_color(rgb(DIM))
                        .child(format!("{step} of 2"))
                })),
        );
    let close = div()
        .id("permission-guide-close")
        .flex_shrink_0()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .text_size(px(15.0))
        .text_color(rgb(DIM))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff14)))
        .on_click(cx.listener(move |view, _, window, cx| on_close(view, window, cx)))
        .child("×");
    div()
        .size_full()
        .p(px(10.0))
        .flex()
        .items_center()
        .gap(px(14.0))
        .rounded(px(14.0))
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgba(0xffffff17))
        .child(tile)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.5))
                        .text_color(rgb(INK))
                        .child(content.title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(DIM))
                        .child(content.detail),
                ),
        )
        .child(status)
        .child(close)
        .into_any_element()
}
