//! Keyboard focus rings in the File diff dialog's controls: they show only after keyboard input
//! (`:focus-visible`, gpui's `last_input_was_keyboard`).
use super::super::native_modal_kit::*;
use gpui::{BoxShadow, FocusHandle, Window, point, px, rgb};

/// Whether `handle` shows its keyboard focus style now.
pub(crate) fn focus_visible(handle: &FocusHandle, window: &Window) -> bool {
    handle.is_focused(window) && window.last_input_was_keyboard()
}

/// shadcn's `--ring` (`oklch(0.556 0 0)`).
const RING: u32 = 0x737373;

/// The 3px `ring-ring/20` halo of a focused shadcn control.
pub(crate) fn focus_ring_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(rgba_of(rgb(RING), 0.2)),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(3.0),
        inset: false,
    }]
}
