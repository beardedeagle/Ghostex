//! The Code view's window tabs: one row above the Code page with a tab per VS Code window (the
//! project's own first, then its extra windows) and a `+` for a new one, the way VS Code shows its
//! windows as tabs on macOS (`window.nativeTabs`). See `app/workarea/code_windows.rs`.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::SharedString;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::view_tab_strip::view_tab_strip_tooltip_text;
use crate::*;

const CODE_WINDOW_TAB_ROW_HEIGHT: f32 = 30.0;
const CODE_WINDOW_TAB_HEIGHT: f32 = 22.0;
/// Room for "New Window" and the close control; below it the row scrolls instead.
const CODE_WINDOW_TAB_MIN_WIDTH: f32 = 132.0;
const CODE_WINDOW_TAB_GROUP: &str = "ghostex-gpui-code-window-tab";

impl GhostexGpuiApp {
    /// The row, whenever Code windows as tabs is on: with only the project's own window it is that
    /// tab and the `+`.
    ///
    /// CDXC:CodeEditor 2026-10-24 DECISION:
    /// User: with the row hidden until a second window existed, and no Code Window row in the side panel's `+` menu, there was no visible way to open one ("what is the way to open a new window now?"). The row stays while the setting is on, so its `+` is always there; this replaces the same day's macOS-style rule that showed it only with two windows or more.
    pub(crate) fn render_code_window_tab_row(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.code_windows_as_tabs_active() {
            return None;
        }
        let project_label: SharedString = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .map(|snapshot| snapshot.display_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| TitlebarMode::Source.tab_label())
            .into();
        let mut tabs =
            vec![self.render_code_window_tab(None, project_label.clone(), project_label, cx)];
        for code_window in &self.code_windows.windows {
            let label = code_window.label();
            let tooltip = code_window.title.clone().unwrap_or_else(|| label.clone());
            tabs.push(self.render_code_window_tab(Some(code_window.id), label, tooltip, cx));
        }
        Some(
            h_flex()
                .id("ghostex-gpui-code-window-tabs")
                .flex_shrink_0()
                .w_full()
                .h(px(CODE_WINDOW_TAB_ROW_HEIGHT))
                .items_center()
                .gap(px(4.0))
                .px(px(6.0))
                .bg(glass_clear(project_editor_shell_background_color()))
                .border_b_1()
                .border_color(titlebar_split_button_border_color())
                .text_color(titlebar_text_color())
                .font_family(crate::ui_fonts::UI_FONT)
                .child(
                    h_flex()
                        .id("ghostex-gpui-code-window-tabs-scroll")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .items_center()
                        .gap(px(4.0))
                        .overflow_x_scroll()
                        .track_scroll(&self.code_window_tab_scroll_handle)
                        .children(tabs),
                )
                .child(self.render_code_window_tab_add_button(cx))
                .into_any_element(),
        )
    }

    /// One window's tab: equal widths down to `CODE_WINDOW_TAB_MIN_WIDTH`, its folder or workspace
    /// in the middle, VS Code's window title as its tooltip, and a close control on the extra
    /// windows only (the project's own window closes with the Code tab).
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// With every tab free to shrink, 16 windows turned each label into an unreadable "…" (measured in Ghostex-3). The tabs now stop at a minimum width and the row scrolls sideways, as the view tab strip does, keeping the shown window's tab in view (`reveal_shown_code_window_tab`).
    fn render_code_window_tab(
        &self,
        id: Option<CodeWindowId>,
        label: SharedString,
        tooltip: SharedString,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let is_showing = self.code_windows.active == id;
        let element_id = id.map_or(0, |id| id.0);
        div()
            .id(format!("ghostex-gpui-code-window-tab-{element_id}"))
            .group(CODE_WINDOW_TAB_GROUP)
            .relative()
            .flex()
            .flex_1()
            .min_w(px(CODE_WINDOW_TAB_MIN_WIDTH))
            .h(px(CODE_WINDOW_TAB_HEIGHT))
            .items_center()
            .justify_center()
            .px(px(22.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(titlebar_split_button_border_color())
            .text_size(px(12.0))
            .cursor_default()
            .when(is_showing, |this| {
                this.bg(titlebar_active_segment_color())
                    .text_color(titlebar_active_text_color())
            })
            .when(!is_showing, |this| {
                this.text_color(titlebar_inactive_text_color())
                    .hover(|this| {
                        this.bg(titlebar_button_hover_color())
                            .text_color(titlebar_active_text_color())
                    })
            })
            // Beside the tab, as the strip's tooltips are: below it would sit behind the page.
            .managed_tooltip_with_placement(
                ManagedTooltipPlacement::WiderSide,
                move |window, cx| {
                    titlebar_tooltip(view_tab_strip_tooltip_text(&tooltip), window, cx)
                },
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.select_code_window(id, window, cx);
                }),
            )
            .when_some(id, |this, id| {
                this.on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.close_code_window(id, window, cx);
                    }),
                )
                .child(self.render_code_window_tab_close_button(id, is_showing, cx))
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .into_any_element()
    }

    /// At the tab's left edge, as macOS draws it: on the showing tab and the hovered one.
    fn render_code_window_tab_close_button(
        &self,
        id: CodeWindowId,
        is_showing: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(format!("ghostex-gpui-code-window-tab-close-{}", id.0))
            .absolute()
            .left(px(4.0))
            .flex()
            .size(px(WORKAREA_VIEW_TAB_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .cursor_default()
            .when(!is_showing, |this| {
                this.opacity(0.0)
                    .group_hover(CODE_WINDOW_TAB_GROUP, |this| this.opacity(1.0))
            })
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_code_window(id, window, cx);
                }),
            )
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_X,
                10.0,
                titlebar_icon_color(),
            ))
    }

    fn render_code_window_tab_add_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .id("ghostex-gpui-code-window-tab-add")
            .flex()
            .flex_shrink_0()
            .size(px(CODE_WINDOW_TAB_HEIGHT))
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_new_code_window(window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                titlebar_tooltip("New Code Window", window, cx)
            })
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_PLUS,
                13.0,
                titlebar_icon_color(),
            ))
    }
}
