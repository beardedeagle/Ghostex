//! The tab row a side panel view draws under the panel's tab strip when it holds more than one
//! thing: Code's VS Code windows (`code_window_tabs.rs`) and Files' open files
//! (`native_docs/open_file_tabs.rs`). The view hands the row its tabs; the row draws them, scrolls
//! them sideways and turns clicks into the view's own select, close, menu and `+` actions.
//!
//! CDXC:Workarea 2026-10-25 WHY:
//! A view that holds several things shows them in its own row under the side panel's strip, and every such view uses this one row, drawn like the strip's own tabs, so its size, close control and scrolling are the same everywhere; the strip keeps one tab per view. The row only draws: the list stays in the view's own model (Code's windows, Files' open files), so moving the panel to a session later changes nothing here.

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
            Self::CodeWindows => "New VS Code Window",
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
            .h(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
            .items_center()
            .gap(px(WORKAREA_VIEW_TAB_GAP))
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
                    .gap(px(WORKAREA_VIEW_TAB_GAP))
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

    /// One tab, drawn as the view tab strip draws its tabs: its label, its tooltip under it, and
    /// the close control after the label.
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// The tabs keep the strip's one width (CDXC:Workarea 2026-09-21 DECISION on `WORKAREA_VIEW_TAB_WIDTH`) instead of shrinking, which turned 16 windows' labels into an unreadable "…", and the row scrolls sideways, keeping the shown tab in view (the view scrolls its handle to it when the shown one changes).
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
            .flex()
            .flex_shrink_0()
            .w(px(WORKAREA_VIEW_TAB_WIDTH))
            .h(px(WORKAREA_VIEW_TAB_HEIGHT))
            .items_center()
            .gap(px(6.0))
            .px(px(WORKAREA_VIEW_TAB_HORIZONTAL_PADDING))
            .rounded(px(WORKAREA_VIEW_TAB_RADIUS))
            .border_1()
            .border_color(titlebar_split_button_border_color())
            .text_size(px(WORKAREA_HEADER_TITLE_TEXT_SIZE))
            .line_height(px(WORKAREA_VIEW_TAB_HEIGHT))
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
            // Under the tab: beside it the bubble covers the neighbouring tab. It draws in the
            // frosted tooltip host's own window, so it sits above the page below the row.
            .managed_tooltip_with_placement(
                ManagedTooltipPlacement::Below,
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
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
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
            .into_any_element()
    }

    /// After the label, as the view tab strip draws it: on the shown tab and the hovered one. An
    /// unsaved tab shows a dot there instead, which turns into the close control on hover.
    fn render_view_instance_tab_close_button(
        &self,
        instance: ViewInstance,
        showing: bool,
        unsaved: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("close")
            .relative()
            .flex()
            .flex_shrink_0()
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
                        11.0,
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
            .size(px(WORKAREA_VIEW_TAB_HEIGHT))
            .items_center()
            .justify_center()
            .rounded(px(WORKAREA_VIEW_TAB_RADIUS))
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
                14.0,
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
