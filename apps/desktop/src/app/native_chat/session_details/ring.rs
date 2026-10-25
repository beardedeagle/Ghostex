//! The context ring of the header's Session details button, painted like the composer's context
//! meter ring (context_meter.rs) but in the header's own colours, so the desktop header and the web
//! header draw the same button.

use gpui::{
    AnyElement, Hsla, IntoElement, ParentElement as _, Styled as _, canvas, div, point, px,
};

/// The ring's percentage, in a box as wide as "100%" so the button keeps its width (and the
/// header its layout) whatever the label says, empty while the context use is not known yet.
pub(crate) fn session_details_ring_label(label: String, color: Hsla) -> AnyElement {
    div()
        .flex_shrink_0()
        .min_w(px(30.0))
        .text_size(px(12.0))
        .text_color(color)
        .child(label)
        .into_any_element()
}

/// A ring `size` pixels wide, `percent` of it drawn in `ink` over a full `track`.
pub(crate) fn session_details_ring(percent: f32, track: Hsla, ink: Hsla, size: f32) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = size * 0.4;
            for (fraction, color) in [(1.0, track), ((percent / 100.0).clamp(0.0, 1.0), ink)] {
                if fraction <= 0.0 {
                    continue;
                }
                let mut path = gpui::PathBuilder::stroke(px((size * 0.125).max(1.5)));
                let steps = (fraction * 96.0).ceil() as usize;
                for step in 0..=steps {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * fraction * step as f32 / steps as f32;
                    let at = center + point(px(angle.cos() * radius), px(angle.sin() * radius));
                    if step == 0 {
                        path.move_to(at);
                    } else {
                        path.line_to(at);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .size(px(size))
    .flex_shrink_0()
    .into_any_element()
}
