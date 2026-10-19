//! The nested shadcn dialog with its absolute close button (`<DialogContent nested
//! showCloseButton>`: Arrange views, Connect your accounts) and the bare backdrop other nested
//! sheets reuse (the extension install consent). dialog.rs draws the variant without the close
//! button in the corner; this one keeps the button at `top-4 right-4` over the header, caps the
//! sheet at a `max-height` with its body (or the whole sheet) scrolling, and keeps a key typed on
//! the sheet from reaching the Settings search behind it.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::dialog::dialog_background;
use super::icon;
use super::row::settings_icon;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, ClickEvent, Context, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    anchored, deferred, div, point, px,
};
use gpui_component::v_flex;

/// `[data-slot='dialog-overlay'] { background: rgb(0 0 0 / 65%) }`.
const BACKDROP_ALPHA: f32 = 0.65;

/// `bg-secondary`: the close button's fill.
pub(crate) fn dialog_close_fill(p: &SettingsPalette) -> gpui::Rgba {
    if p.glass && !p.light {
        css_mix(gpui::rgb(0xffffff), 0.06, p.modal.solid_surface)
    } else if p.light {
        gpui::rgb(0xf1f1f1)
    } else {
        gpui::rgb(0x27272a)
    }
}

/// The window-wide backdrop with `content` centered on it; a press on the backdrop runs
/// `on_dismiss` (Base UI closes a dialog on an outside press).
pub(crate) fn settings_dialog_overlay<V: 'static>(
    id: impl Into<SharedString>,
    content: AnyElement,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_dialog_layer(id, content, BACKDROP_ALPHA, on_dismiss, window, cx)
}

/// [`settings_dialog_overlay`] with its own backdrop tint (an `AppModalShell` nested in Settings, the
/// extension install consent, dims nothing: `backdrop_alpha` 0 still catches the outside press).
pub(crate) fn settings_dialog_layer<V: 'static>(
    id: impl Into<SharedString>,
    content: AnyElement,
    backdrop_alpha: f32,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let id: SharedString = id.into();
    let viewport = window.viewport_size();
    deferred(
        anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(point(px(0.0), px(0.0)))
            .child(
                div()
                    .id(SharedString::from(format!("{id}-backdrop")))
                    .occlude()
                    .w(viewport.width)
                    .h(viewport.height)
                    .bg(hsla(modal_rgba(0x000000, backdrop_alpha)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |page, _: &MouseDownEvent, window, cx| {
                            on_dismiss(page, window, cx);
                        }),
                    )
                    .child(content),
            ),
    )
    .with_priority(2)
    .into_any_element()
}

/// The nested dialog. `focus` is tracked on the sheet (focus it when the dialog opens) so Escape
/// closes it. `scroll_all` scrolls the whole sheet (the account guide); otherwise the body is a
/// flexible column whose own children scroll (the view order list).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_dialog_sheet<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    description: Option<AnyElement>,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    scroll_all: bool,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    dialog_sheet(
        p,
        id,
        focus,
        title,
        description,
        width,
        max_height,
        padding,
        gap,
        scroll_all,
        body,
        footer,
        on_close,
        SheetClose::Square,
        window,
        cx,
    )
}

/// [`settings_dialog_sheet`] with the Settings window's own close button: the round X centred on
/// the sheet's top-right corner, shown while the pointer is over the sheet's top band
/// (`modal_corner_close_button`, the one window/modal_window_frame.rs draws for app modals).
///
/// CDXC:AppModal 2026-10-19 DECISION:
/// User, on the Orchestrator instructions editor nested in Settings: "please make the x button on the top right of this appear just like the x button we have for the settings modal itself". The sheet reuses the modal's corner X (same component, size, icon, colours, hover and corner position, revealed from the top band) instead of the filled square; the other nested sheets keep the square until the user asks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_dialog_sheet_with_corner_close<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    description: Option<AnyElement>,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
    close_always_shown: bool,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let close = if close_always_shown {
        SheetClose::CornerShown
    } else {
        SheetClose::Corner
    };
    dialog_sheet(
        p,
        id,
        focus,
        title,
        description,
        width,
        max_height,
        padding,
        gap,
        false,
        body,
        footer,
        on_close,
        close,
        window,
        cx,
    )
}

/// Which close button a sheet draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SheetClose {
    /// The filled square at `top-4 right-4`.
    Square,
    /// The modal's corner X, revealed from the top band.
    Corner,
    /// The corner X shown at rest (the preview binary's screenshots, which cannot hover).
    CornerShown,
}

#[allow(clippy::too_many_arguments)]
fn dialog_sheet<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    description: Option<AnyElement>,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    scroll_all: bool,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
    close_kind: SheetClose,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let id: SharedString = id.into();
    let corner_close = close_kind != SheetClose::Square;
    // The band that reveals the corner X: the sheet's top padding and its header.
    let band: SharedString = format!("{id}-top-band").into();
    let close_fill = dialog_close_fill(p);
    let close_hover = css_mix(p.foreground, 0.05, close_fill);
    let close = if corner_close {
        let on_close = on_close.clone();
        let half = MODAL_CORNER_CLOSE_SIZE / 2.0;
        modal_corner_close_button(
            &p.modal,
            SharedString::from(format!("{id}-close")),
            if close_kind == SheetClose::CornerShown {
                ModalCornerCloseReveal::Shown
            } else {
                ModalCornerCloseReveal::OnGroupHover(band.clone())
            },
            move |page: &mut V, window, cx| on_close(page, window, cx),
            cx,
        )
        .absolute()
        .top(px(-half))
        .right(px(-half))
        .size(px(MODAL_CORNER_CLOSE_SIZE))
        // The half outside the sheet sits over the backdrop, whose press would close the sheet
        // on mouse-down instead of on the button's own click.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .into_any_element()
    } else {
        let on_close = on_close.clone();
        div()
            .id(SharedString::from(format!("{id}-close")))
            .absolute()
            .top(px(16.0))
            .right(px(16.0))
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .bg(hsla(close_fill))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(close_hover)))
            .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                on_close(page, window, cx);
            }))
            .child(settings_icon(icon::X, 16.0, p.foreground))
            .into_any_element()
    };
    let header = v_flex()
        .id(band.clone())
        .group(band)
        .w_full()
        .when(corner_close, |this| this.pt(px(padding)))
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child(title.into()),
        )
        .children(description.map(|description| {
            div()
                .w_full()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.muted))
                .child(description)
        }));
    let focus_for_keys = focus.clone();
    let on_escape = on_close.clone();
    let viewport = window.viewport_size();
    let content = v_flex()
        .w_full()
        .gap(px(gap))
        .when(!scroll_all, |this| this.flex_1().min_h_0())
        .child(header)
        .child(
            v_flex()
                .w_full()
                .gap(px(gap))
                .when(!scroll_all, |this| this.flex_1().min_h_0())
                .children(body),
        )
        .children(footer);
    let sheet = div()
        .id(id.clone())
        .track_focus(focus)
        .occlude()
        .relative()
        .flex()
        .flex_col()
        .w(px(width))
        .max_w(viewport.width - px(32.0))
        .max_h(px(max_height.unwrap_or(f32::MAX)).min(viewport.height - px(32.0)))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(dialog_background(p)))
        .shadow_xl()
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(cx.listener(move |page, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                on_escape(page, window, cx);
                return;
            }
            if focus_for_keys.is_focused(window) {
                cx.stop_propagation();
            }
        }))
        .child(if scroll_all {
            div()
                .id(SharedString::from(format!("{id}-scroll")))
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(padding))
                .child(content)
                .into_any_element()
        } else {
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                // With the corner X the header carries the top padding, so the band that reveals
                // the X starts at the sheet's top edge.
                .px(px(padding))
                .pb(px(padding))
                .when(!corner_close, |this| this.pt(px(padding)))
                .child(content)
                .into_any_element()
        })
        .child(close)
        .into_any_element();
    settings_dialog_overlay(
        id,
        sheet,
        move |page, window, cx| on_close(page, window, cx),
        window,
        cx,
    )
}
