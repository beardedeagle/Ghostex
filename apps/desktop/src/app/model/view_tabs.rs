//! The value types the view panel's tab strip drags around: what is being dragged, where it would
//! land, and the little tab that follows the pointer.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::*;

/// The payload GPUI carries while a view tab is being dragged along the strip.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct DraggedViewTab {
    pub(crate) mode: TitlebarMode,
}

/// One tab of the view panel's strip, whichever kind it is: a view, or one of the Browser's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ViewStripTabKey {
    View(TitlebarMode),
    Browser(BrowserTabId),
}

impl ViewStripTabKey {
    fn to_shell_state_string(self) -> String {
        match self {
            Self::View(mode) => format!("view:{}", mode.element_slug()),
            Self::Browser(tab_id) => format!("browser:{}", tab_id.0),
        }
    }

    /// The Browser's own key is kept: it places the strip's Browser tab while its pages are in a
    /// row of their own (`browserPagesInViewRow`), and is skipped while they are not.
    fn from_shell_state_string(value: &str) -> Option<Self> {
        if let Some(slug) = value.strip_prefix("view:") {
            return TitlebarMode::from_slug(slug)
                .filter(|mode| *mode != TitlebarMode::Agents)
                .map(Self::View);
        }
        value
            .strip_prefix("browser:")
            .and_then(|id| id.parse::<u64>().ok())
            .map(|id| Self::Browser(BrowserTabId(id)))
    }
}

/// CDXC:Workarea 2026-09-21 DECISION:
/// User (ruling 1A): the strip is one row in one order, "any tab can be dragged anywhere, views and
/// pages mixed", and (ruling 4B) tabs can be pinned: icon-only, kept at the left, not closable by
/// accident. `open_views` and the Browser's panes still say which tabs exist; this only says where
/// each is drawn and which are pinned. A tab it has not seen yet lands at the end. It is
/// project-owned like the open views are, because browser tab ids are per project. With
/// `browserPagesInViewRow` on (off by default) the pages are drawn and dragged in their own row
/// (`render/browser_page_tabs.rs`) but keep their places in this one order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GpuiViewStripLayout {
    pub(crate) order: Vec<ViewStripTabKey>,
    pub(crate) pinned: Vec<ViewStripTabKey>,
}

impl GpuiViewStripLayout {
    /// `available` (the tabs to draw, in their natural order) in drawn order: pinned tabs first,
    /// then the rest, each group in the stored order, tabs it has not placed after the ones it has.
    pub(crate) fn in_drawn_order<T>(
        &self,
        mut available: Vec<T>,
        key: impl Fn(&T) -> ViewStripTabKey,
    ) -> Vec<T> {
        let mut ordered = Vec::with_capacity(available.len());
        for stored in &self.order {
            if let Some(position) = available.iter().position(|tab| key(tab) == *stored) {
                ordered.push(available.remove(position));
            }
        }
        ordered.append(&mut available);
        let (pinned, unpinned): (Vec<_>, Vec<_>) = ordered
            .into_iter()
            .partition(|tab| self.pinned.contains(&key(tab)));
        pinned.into_iter().chain(unpinned).collect()
    }

    pub(crate) fn to_shell_state_json(&self) -> serde_json::Value {
        let keys = |keys: &[ViewStripTabKey]| {
            keys.iter()
                .map(|key| serde_json::Value::String(key.to_shell_state_string()))
                .collect::<Vec<_>>()
        };
        serde_json::json!({
            "order": keys(&self.order),
            "pinned": keys(&self.pinned),
        })
    }

    pub(crate) fn from_shell_state(value: Option<&serde_json::Value>) -> Self {
        let keys = |field: &str| {
            let mut keys = Vec::new();
            let entries = value
                .and_then(|value| value.get(field))
                .and_then(serde_json::Value::as_array);
            for entry in entries.into_iter().flatten() {
                if let Some(key) = entry
                    .as_str()
                    .and_then(ViewStripTabKey::from_shell_state_string)
                    && !keys.contains(&key)
                {
                    keys.push(key);
                }
            }
            keys
        };
        Self {
            order: keys("order"),
            pinned: keys("pinned"),
        }
    }
}

/// The tab that follows the pointer during a reorder. It is a plain label, not the live tab, because
/// the live tab keeps rendering in the strip while it is dragged.
pub(crate) struct ViewTabDragPreview {
    pub(crate) icon: &'static str,
    pub(crate) label: String,
}

impl Render for ViewTabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .h(px(WORKAREA_VIEW_TAB_HEIGHT))
            .w(px(WORKAREA_VIEW_TAB_WIDTH))
            .items_center()
            .gap(px(6.0))
            .overflow_hidden()
            .rounded(px(WORKAREA_VIEW_TAB_RADIUS))
            .border_1()
            .border_color(workspace_drop_feedback_border_color())
            .bg(workspace_tab_drag_preview_color())
            .px(px(WORKAREA_VIEW_TAB_HORIZONTAL_PADDING))
            .text_size(px(12.5))
            .text_color(titlebar_active_text_color())
            .shadow_md()
            .child(titlebar_svg_icon(
                self.icon,
                WORKAREA_VIEW_TAB_ICON_SIZE,
                titlebar_icon_color(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.label.clone()),
            )
    }
}
