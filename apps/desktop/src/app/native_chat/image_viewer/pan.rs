//! Dragging the previewed picture to move around it, and keeping the spot a zoom step was taken at
//! where it was.

use super::window::ImageViewerWindow;
use gpui::{
    Bounds, Context, DispatchPhase, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Size, Styled as _, canvas, point, px,
};

/// How far a press on the picture travels before it is a drag instead of a zoom click.
const DRAG_THRESHOLD: f32 = 4.0;

/**
 * A press on the picture that has not been released yet.
 *
 * CDXC:SessionChat 2026-09-27 DECISION:
 * User: dragging the previewed image pans around it. A press that travels less than
 * `DRAG_THRESHOLD` is still the click that steps the zoom, so the zoom now happens on release;
 * one that travels further only moves the picture, under a closed-hand cursor while it does.
 */
pub(super) struct PicturePress {
    start: Point<Pixels>,
    /// The scroll offset when the press began: a drag moves the picture by the pointer's travel.
    offset: Point<Pixels>,
    panning: bool,
}

/// The spot on the picture a zoom step was asked for, as fractions of its size, and the window
/// point it stays under once the picture has its new size.
pub(super) struct ZoomAnchor {
    fraction: Point<f32>,
    at: Point<Pixels>,
}

/// Where the stage puts a picture of `painted` size inside the scroll box at offset zero: centred
/// while it is smaller than the box, at the edge once it is bigger.
pub(crate) fn stage_inset(scroll: Bounds<Pixels>, painted: Size<Pixels>) -> Point<Pixels> {
    let inset = |free: Pixels| (free / 2.0).max(px(0.0));
    point(
        inset(scroll.size.width - painted.width),
        inset(scroll.size.height - painted.height),
    )
}

impl ImageViewerWindow {
    pub(super) fn press_picture(&mut self, position: Point<Pixels>) {
        self.press = Some(PicturePress {
            start: position,
            offset: self.scroll.offset(),
            panning: false,
        });
    }

    fn drag_picture(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(press) = &mut self.press else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            // Released somewhere this window never heard about.
            self.press = None;
            cx.notify();
            return;
        }
        let travel = event.position - press.start;
        if !press.panning {
            if f32::from(travel.x).hypot(f32::from(travel.y)) < DRAG_THRESHOLD {
                return;
            }
            press.panning = true;
            // Repaint for the closed hand even when the picture has nowhere to move.
            cx.notify();
        }
        let max = self.scroll.max_offset();
        let next = press.offset + travel;
        let next = point(next.x.clamp(-max.x, px(0.0)), next.y.clamp(-max.y, px(0.0)));
        if next != self.scroll.offset() {
            self.scroll.set_offset(next);
            cx.notify();
        }
    }

    fn release_picture(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        match self.press.take() {
            Some(press) if press.panning => cx.notify(),
            Some(_) => self.zoom_at(event.position, cx),
            None => {}
        }
    }

    /**
     * Steps the zoom, keeping the spot of the picture under `at` there.
     *
     * CDXC:SessionChat 2026-09-27 WHY:
     * React's viewer scrolled the enlarged picture to the clicked spot. Without it the stage,
     * which starts a picture bigger than the viewer at its edge so every part can be panned to,
     * would open each zoom step at the picture's top-left corner.
     */
    pub(super) fn zoom_at(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.zoom_anchor = self.painted.map(|painted| {
            let scroll = self.scroll.bounds();
            let origin = scroll.origin + self.scroll.offset() + stage_inset(scroll, painted);
            let fraction = |from: Pixels, size: Pixels| {
                if size > px(0.0) {
                    (f32::from(from) / f32::from(size)).clamp(0.0, 1.0)
                } else {
                    0.5
                }
            };
            ZoomAnchor {
                fraction: point(
                    fraction(at.x - origin.x, painted.width),
                    fraction(at.y - origin.y, painted.height),
                ),
                at,
            }
        });
        self.chat.update(cx, |chat, cx| chat.zoom_image_viewer(cx));
    }

    /// Called while rendering the picture at `painted`: scrolls a pending zoom step's spot back
    /// under the point it was asked for, and remembers the size for the next step.
    pub(super) fn follow_zoom_anchor(&mut self, painted: Option<Size<Pixels>>) {
        if let (Some(anchor), Some(painted)) = (self.zoom_anchor.take(), painted) {
            let scroll = self.scroll.bounds();
            let inset = stage_inset(scroll, painted);
            let offset =
                |at: Pixels, start: Pixels, inset: Pixels, spot: Pixels, overflow: Pixels| {
                    (at - start - inset - spot).clamp(-overflow.max(px(0.0)), px(0.0))
                };
            self.scroll.set_offset(point(
                offset(
                    anchor.at.x,
                    scroll.origin.x,
                    inset.x,
                    painted.width * anchor.fraction.x,
                    painted.width - scroll.size.width,
                ),
                offset(
                    anchor.at.y,
                    scroll.origin.y,
                    inset.y,
                    painted.height * anchor.fraction.y,
                    painted.height - scroll.size.height,
                ),
            ));
        }
        self.painted = painted;
    }

    /**
     * Window-wide move and release listeners for a press on the picture, and the closed hand while
     * it pans. The pointer leaves the picture, and often the viewer, mid-drag, where element
     * listeners and cursors stop applying. A canvas registers no hitbox, so this takes no input
     * from anything under it.
     */
    pub(super) fn pan_listeners(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        let panning = self.press.as_ref().is_some_and(|press| press.panning);
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                if panning {
                    window.set_window_cursor_style(gpui::CursorStyle::ClosedHand);
                }
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moved.update(cx, |this, cx| this.drag_picture(event, cx));
                    }
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        let _ = view.update(cx, |this, cx| this.release_picture(event, cx));
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}
