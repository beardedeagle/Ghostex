//! The previous and next arrows at the viewer's sides, shown while the pointer is near that side
//! and there is more than one picture to step through.
//!
//! CDXC:SessionChat 2026-10-22 DECISION:
//! User: "left and right arrows that appear when the pointer comes near the viewer's left or right edge", alongside the Left and Right keys, move to the previous or next picture. Asked for the floating page window's pictures; the viewer is the transcript's own, so a message with several pictures gets the same arrows.

use super::super::appearance::ChatAppearance;
use super::window::ImageViewerWindow;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, DispatchPhase, InteractiveElement as _, IntoElement, MouseMoveEvent,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, canvas, div, px, svg,
};
use gpui_component::tooltip::Tooltip;

/// How close to a side the pointer has to come for that side's arrow to show.
const EDGE_REACH: f32 = 120.0;
const ARROW_SIZE: f32 = 44.0;
const ARROW_INSET: f32 = 16.0;

impl ImageViewerWindow {
    /// The side the pointer is near: -1 left, 1 right, 0 neither.
    fn note_pointer(&mut self, x: Pixels, width: Pixels, cx: &mut Context<Self>) {
        let edge = if x < px(EDGE_REACH) {
            -1
        } else if x > width - px(EDGE_REACH) {
            1
        } else {
            0
        };
        if edge != self.edge {
            self.edge = edge;
            cx.notify();
        }
    }

    /// The two arrows and the window-wide pointer listener that shows them; nothing when there is
    /// only one picture.
    pub(super) fn step_arrows(
        &self,
        count: usize,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if count < 2 {
            return None;
        }
        let view = cx.weak_entity();
        // A canvas registers no hitbox, so it hears every move without taking input from anything.
        let listener = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let width = window.viewport_size().width;
                        let _ = view.update(cx, |this, cx| {
                            this.note_pointer(event.position.x, width, cx)
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let surface = if p.light {
            gpui::white().opacity(0.9)
        } else {
            gpui::black().opacity(0.5)
        };
        let arrow = |delta: isize, cx: &Context<Self>| {
            let (id, label, icon) = if delta < 0 {
                (
                    "chat-image-previous",
                    "Previous image (←)",
                    "titlebar/chevron-left.svg",
                )
            } else {
                (
                    "chat-image-next",
                    "Next image (→)",
                    "titlebar/chevron-right.svg",
                )
            };
            // A plain column takes no input, so only the button itself is in the way.
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .when(delta < 0, |this| this.left(px(ARROW_INSET)))
                .when(delta > 0, |this| this.right(px(ARROW_INSET)))
                .flex()
                .items_center()
                .child(
                    div()
                        .id(id)
                        .role(gpui::Role::Button)
                        .aria_label(label)
                        .size(px(ARROW_SIZE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(surface)
                        .when(p.light, |this| this.border_1().border_color(p.border))
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.input))
                        .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                        // The surround closes the viewer on a press; the arrow keeps it.
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.stop_propagation()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.chat
                                .update(cx, |chat, cx| chat.step_image_viewer(delta, cx));
                        }))
                        .child(svg().path(icon).size(px(22.0)).text_color(if p.light {
                            p.foreground
                        } else {
                            p.primary
                        })),
                )
        };
        Some(
            div()
                .absolute()
                .size_full()
                .child(listener)
                .when(self.edge < 0, |this| this.child(arrow(-1, cx)))
                .when(self.edge > 0, |this| this.child(arrow(1, cx)))
                .into_any_element(),
        )
    }
}
