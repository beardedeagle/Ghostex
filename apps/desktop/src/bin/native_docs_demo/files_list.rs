//! The two header pieces of `native_docs/files_list.rs` the picture zoom draws; the real file
//! needs the whole app. Same sizes as the real ones (consts.rs).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, px, svg,
};

use super::palette::DocsPalette;

pub(crate) fn header_tile(
    id: impl Into<gpui::ElementId>,
    icon: impl IntoElement,
    active: bool,
    disabled: bool,
    p: &DocsPalette,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(32.0))
        .h(px(27.0))
        .rounded(px(7.0))
        .when(active, |this| this.bg(hover))
        .when(!disabled, |this| {
            this.cursor_pointer().hover(move |style| style.bg(hover))
        })
        .child(icon)
}

pub(crate) fn header_icon(path: &'static str, disabled: bool, p: &DocsPalette) -> AnyElement {
    svg()
        .size(px(18.0))
        .path(path)
        .text_color(if disabled {
            p.toolbar_disabled
        } else {
            p.toolbar_icon
        })
        .into_any_element()
}
