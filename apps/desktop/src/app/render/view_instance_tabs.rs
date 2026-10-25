//! The tab row a side panel view draws under the panel's tab strip when it holds more than one
//! thing: Code's VS Code windows (`code_window_tabs.rs`) and Files' open files
//! (`native_docs/open_file_tabs.rs`). The view hands the row its tabs; the row draws them, scrolls
//! them sideways and turns clicks into the view's own select, close, menu and `+` actions.
//!
//! CDXC:Workarea 2026-10-25 DECISION:
//! User (panel tabs decisions K1, K6): a view that holds several things shows them in its own row under the side panel's strip, and every such view uses this one row, so its size, close control and scrolling are the same everywhere; the strip keeps one tab per view. The row only draws: the list stays in the view's own model (Code's windows, Files' open files), so moving the panel to a session later changes nothing here.

use gpui::AnyElement;
use gpui::ElementId;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::ScrollHandle;
use gpui::SharedString;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::view_tab_strip::view_tab_strip_tooltip_text;
use crate::*;

const VIEW_INSTANCE_TAB_ROW_HEIGHT: f32 = 30.0;
const VIEW_INSTANCE_TAB_HEIGHT: f32 = 22.0;
/// Room for "New Window" and the close control; below it the row scrolls instead.
const VIEW_INSTANCE_TAB_MIN_WIDTH: f32 = 132.0;
const VIEW_INSTANCE_TAB_GROUP: &str = "ghostex-gpui-view-instance-tab";

/// The view a row belongs to, which decides what its `+` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewInstanceRow {
    CodeWindows,
    OpenFiles,
}

impl ViewInstanceRow {
    fn element_id(self) -> &'static str {
        match self {
            Self::CodeWindows => "ghostex-gpui-code-window-tabs",
            Self::OpenFiles => "ghostex-gpui-open-file-tabs",
        }
    }

    fn add_tooltip(self) -> &'static str {
        match self {
            Self::CodeWindows => "New Code Window",
            Self::OpenFiles => "Open File",
        }
    }

    /// A right click on a tab opens the view's tab menu (`show_view_instance_menu`).
    fn has_tab_menu(self) -> bool {
        matches!(self, Self::OpenFiles)
    }
}

/// What a tab stands for, handed back to its view's actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ViewInstance {
    /// A Code window; `None` is the project's own.
    CodeWindow(Option<CodeWindowId>),
    /// A file open in Files, by path.
    OpenFile(SharedString),
}

impl ViewInstance {
    /// The tab's element id comes from what it stands for, not its place in the row, so the
    /// hover and tooltip state a tab keeps stays with that window or file when another one closes.
    fn element_id(&self) -> ElementId {
        match self {
            Self::CodeWindow(id) => ElementId::NamedInteger(
                SharedString::new_static("code-window"),
                id.map_or(0, |id| id.0),
            ),
            Self::OpenFile(path) => ElementId::Name(path.clone()),
        }
    }
}

pub(crate) struct ViewInstanceTab {
    pub(crate) instance: ViewInstance,
    pub(crate) label: SharedString,
    pub(crate) tooltip: SharedString,
    pub(crate) showing: bool,
    /// Has a close control and closes on a middle click (the project's own Code window closes
    /// with the Code tab instead).
    pub(crate) closable: bool,
    /// Unsaved changes: a dot in the close control's place until the tab is hovered.
    pub(crate) unsaved: bool,
}

/// `body` with the row above it as its sibling, never over it; just `body` without a row.
pub(crate) fn with_view_instance_tab_row(row: Option<AnyElement>, body: AnyElement) -> AnyElement {
    let Some(row) = row else {
        return body;
    };
    v_flex()
        .size_full()
        .min_w_0()
        .min_h_0()
        .child(row)
        .child(div().relative().flex_1().min_w_0().min_h_0().child(body))
        .into_any_element()
}

impl GhostexGpuiApp {
    pub(crate) fn render_view_instance_tab_row(
        &self,
        row: ViewInstanceRow,
        tabs: impl IntoIterator<Item = ViewInstanceTab>,
        scroll: &ScrollHandle,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        h_flex()
            .id(row.element_id())
            .flex_shrink_0()
            .w_full()
            .h(px(VIEW_INSTANCE_TAB_ROW_HEIGHT))
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
                    .id("ghostex-gpui-view-instance-tabs-scroll")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .gap(px(4.0))
                    .overflow_x_scroll()
                    .track_scroll(scroll)
                    .children(
                        tabs.into_iter()
                            .map(|tab| self.render_view_instance_tab(row, tab, cx)),
                    ),
            )
            .child(self.render_view_instance_tab_add_button(row, cx))
            .into_any_element()
    }

    /// One tab: equal widths down to `VIEW_INSTANCE_TAB_MIN_WIDTH`, its label in the middle, its
    /// tooltip beside it, and the close control at its left edge.
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// With every tab free to shrink, 16 windows turned each label into an unreadable "…" (measured in Ghostex-3). The tabs stop at a minimum width and the row scrolls sideways, as the view tab strip does, keeping the shown tab in view (the view scrolls its handle to it when the shown one changes).
    fn render_view_instance_tab(
        &self,
        row: ViewInstanceRow,
        tab: ViewInstanceTab,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let ViewInstanceTab {
            instance,
            label,
            tooltip,
            showing,
            closable,
            unsaved,
        } = tab;
        let select = instance.clone();
        div()
            .id(instance.element_id())
            .group(VIEW_INSTANCE_TAB_GROUP)
            .relative()
            .flex()
            .flex_1()
            .min_w(px(VIEW_INSTANCE_TAB_MIN_WIDTH))
            .h(px(VIEW_INSTANCE_TAB_HEIGHT))
            .items_center()
            .justify_center()
            .px(px(22.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(titlebar_split_button_border_color())
            .text_size(px(12.0))
            .cursor_default()
            .when(showing, |this| {
                this.bg(titlebar_active_segment_color())
                    .text_color(titlebar_active_text_color())
            })
            .when(!showing, |this| {
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
                    this.select_view_instance(&select, window, cx);
                }),
            )
            .when(row.has_tab_menu(), |this| {
                let menu = instance.clone();
                this.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_view_instance_menu(&menu, event.position, window, cx);
                    }),
                )
            })
            .when(closable, |this| {
                let middle = instance.clone();
                this.on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.close_view_instance(&middle, window, cx);
                    }),
                )
                .child(self.render_view_instance_tab_close_button(instance, showing, unsaved, cx))
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

    /// At the tab's left edge, as macOS draws it: on the shown tab and the hovered one. An unsaved
    /// tab shows a dot there instead, which turns into the close control on hover.
    fn render_view_instance_tab_close_button(
        &self,
        instance: ViewInstance,
        showing: bool,
        unsaved: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("close")
            .absolute()
            .left(px(4.0))
            .flex()
            .size(px(WORKAREA_VIEW_TAB_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .cursor_default()
            .when(!showing && !unsaved, |this| {
                this.opacity(0.0)
                    .group_hover(VIEW_INSTANCE_TAB_GROUP, |this| this.opacity(1.0))
            })
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_view_instance(&instance, window, cx);
                }),
            )
            .child(
                div()
                    .when(unsaved, |this| {
                        this.invisible()
                            .group_hover(VIEW_INSTANCE_TAB_GROUP, |this| this.visible())
                    })
                    .child(titlebar_svg_icon(
                        TITLEBAR_ICON_X,
                        10.0,
                        titlebar_icon_color(),
                    )),
            )
            .when(unsaved, |this| {
                this.child(
                    div()
                        .absolute()
                        .size(px(7.0))
                        .rounded_full()
                        .bg(titlebar_icon_color())
                        .group_hover(VIEW_INSTANCE_TAB_GROUP, |this| this.invisible()),
                )
            })
    }

    fn render_view_instance_tab_add_button(
        &self,
        row: ViewInstanceRow,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("ghostex-gpui-view-instance-tab-add")
            .flex()
            .flex_shrink_0()
            .size(px(VIEW_INSTANCE_TAB_HEIGHT))
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    match row {
                        ViewInstanceRow::CodeWindows => this.open_new_code_window(window, cx),
                        ViewInstanceRow::OpenFiles => this.native_docs_open_file_prompt(window, cx),
                    }
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, move |window, cx| {
                titlebar_tooltip(row.add_tooltip(), window, cx)
            })
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_PLUS,
                13.0,
                titlebar_icon_color(),
            ))
    }

    fn select_view_instance(
        &mut self,
        instance: &ViewInstance,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match instance {
            ViewInstance::CodeWindow(id) => self.select_code_window(*id, window, cx),
            ViewInstance::OpenFile(path) => self.native_docs_select_tab(path, window, cx),
        }
    }

    fn close_view_instance(
        &mut self,
        instance: &ViewInstance,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match instance {
            ViewInstance::CodeWindow(Some(id)) => self.close_code_window(*id, window, cx),
            ViewInstance::CodeWindow(None) => {}
            ViewInstance::OpenFile(path) => self.native_docs_request_close(path, window, cx),
        }
    }

    fn show_view_instance_menu(
        &mut self,
        instance: &ViewInstance,
        position: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let ViewInstance::OpenFile(path) = instance {
            self.show_native_docs_tab_menu(path, position, window, cx);
        }
    }
}
