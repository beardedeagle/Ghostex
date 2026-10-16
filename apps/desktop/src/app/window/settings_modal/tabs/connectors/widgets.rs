//! The small pieces the Connectors sections share: a muted detail line and a note row.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{ROW_PADDING_X, ROW_PADDING_Y};
use super::super::super::palette::SettingsPalette;
use gpui::{AnyElement, IntoElement, ParentElement as _, Styled as _, div, px};

/// A list item's one-line detail.
pub(super) fn detail(text: impl Into<String>) -> AnyElement {
    div().child(text.into()).into_any_element()
}

/// A muted line under a section's rows: what to do next after an action.
pub(super) fn note_row(p: &SettingsPalette, text: &str) -> AnyElement {
    div()
        .w_full()
        .px(px(ROW_PADDING_X))
        .py(px(ROW_PADDING_Y))
        .text_size(px(13.0))
        .line_height(px(18.2))
        .text_color(hsla(p.muted))
        .child(text.to_string())
        .into_any_element()
}
