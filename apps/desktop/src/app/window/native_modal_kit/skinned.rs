use super::*;
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, InteractiveElement as _, IntoElement,
    ParentElement as _, Rgba, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex};

/// A modal-specific field skin for select triggers and text areas whose
/// stylesheet restates the shared shadcn look (Add Worktree: `#161616` fields
/// on a 8% hairline with a 28% focus edge at 13px).
#[derive(Clone, Copy)]
pub(crate) struct ModalFieldSkin {
    pub(crate) background: Rgba,
    pub(crate) border: Rgba,
    pub(crate) focus_border: Rgba,
    pub(crate) text_size: f32,
}

/// [`modal_select_trigger`] with a [`ModalFieldSkin`] and a caller-chosen
/// chevron (the searchable React trigger draws `IconChevronDown`, the plain one
/// `IconSelector`). Full width, no hover fill, the focus edge while open.
pub(crate) fn modal_select_trigger_skinned<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    value: Option<String>,
    placeholder: &'static str,
    skin: ModalFieldSkin,
    chevron: &'static str,
    disabled: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let open = select.open;
    h_flex()
        .id(id)
        .w_full()
        .flex_shrink_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_between()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if open { skin.focus_border } else { skin.border }))
        .bg(hsla(skin.background))
        .text_size(px(skin.text_size))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_toggle(this, window, cx);
                }))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(value.is_none(), |this| this.text_color(hsla(p.muted)))
                .child(value.unwrap_or_else(|| placeholder.to_string())),
        )
        .child(
            div()
                .flex_shrink_0()
                .child(modal_icon(chevron, 16.0, p.muted)),
        )
        .into_any_element()
}

/// A text field's paste hook: gets the pasted clipboard and returns whether it took the paste.
pub(crate) type ModalPasteHook =
    std::rc::Rc<dyn Fn(&gpui::ClipboardItem, &mut Window, &mut App) -> bool>;

/// [`modal_text_area`] with a [`ModalFieldSkin`]. The box keeps `min_height`
/// and grows with the editor, so create the `TextareaState` with
/// `.auto_grow(min_rows, max_rows)` to cap the growth the way the React
/// `max-height` does. `on_paste` is the field's paste hook ([`Textarea::on_paste`]).
pub(crate) fn modal_text_area_skinned(
    p: &ModalPalette,
    state: &gpui::Entity<TextareaState>,
    skin: ModalFieldSkin,
    min_height: f32,
    disabled: bool,
    on_paste: Option<ModalPasteHook>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .min_h(px(min_height))
        .px(px(12.0))
        .py(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused {
            skin.focus_border
        } else {
            skin.border
        }))
        .bg(hsla(skin.background))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div().w_full().min_w_0().child(
                Textarea::new(state)
                    .when_some(on_paste, |this, on_paste| {
                        this.on_paste(move |item, window, cx| on_paste(item, window, cx))
                    })
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .disabled(disabled)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(skin.text_size))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .into_any_element()
}

/// [`modal_text_input`] with a [`ModalFieldSkin`] and a caller-chosen side padding (the shadcn
/// `Input` is `px-2.5`), 32px tall, its border turning `focus_border` while focused.
pub(crate) fn modal_text_input_skinned(
    p: &ModalPalette,
    state: &gpui::Entity<InputState>,
    skin: ModalFieldSkin,
    padding_x: f32,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(padding_x))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused {
            skin.focus_border
        } else {
            skin.border
        }))
        .bg(hsla(skin.background))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(skin.text_size))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .into_any_element()
}

/// The fill, edge and ink of one shadcn `Button` variant inside a modal's skin, at rest and hovered.
#[derive(Clone, Copy)]
pub(crate) struct ModalButtonSkin {
    pub(crate) background: Rgba,
    pub(crate) border: Rgba,
    pub(crate) text: Rgba,
    pub(crate) hover_background: Rgba,
    pub(crate) hover_text: Rgba,
    pub(crate) pressed_background: Rgba,
}

/// A shadcn `Button` (packages/components/ui/button.tsx (deleted 2026-10-01)): content-sized, 8px radius, a 1px edge,
/// `height` tall (32 for `default` and `icon`, 28 for `sm`) with `padding_x` sides, a 14px/400
/// label after an optional leading icon, `gap` apart, and `disabled_opacity` when disabled
/// (0.5 in shadcn). A button without a label is square. Returned stateful so callers can attach
/// a tooltip.
#[allow(clippy::too_many_arguments)]
pub(crate) fn modal_skinned_button<V: 'static>(
    id: impl Into<gpui::ElementId>,
    skin: ModalButtonSkin,
    height: f32,
    padding_x: f32,
    gap: f32,
    leading: Option<AnyElement>,
    label: Option<SharedString>,
    disabled: bool,
    disabled_opacity: f32,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let square = label.is_none();
    h_flex()
        .id(id)
        .flex_shrink_0()
        .h(px(height))
        .when(square, |this| this.w(px(height)))
        .when(!square, |this| this.px(px(padding_x)))
        .gap(px(gap))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(skin.border))
        .bg(hsla(skin.background))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(skin.text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(disabled_opacity))
        .when(!disabled, |this| {
            this.hover(move |this| {
                this.bg(hsla(skin.hover_background))
                    .text_color(hsla(skin.hover_text))
            })
            .active(move |this| this.bg(hsla(skin.pressed_background)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                on_click(this, window, cx);
            }))
        })
        .children(leading)
        .children(label)
}
