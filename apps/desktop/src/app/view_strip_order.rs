//! The view panel's strip as one ordered row: which tab is drawn where, which are pinned, and what
//! a drag along the row does.

use crate::app::model::*;
use crate::app::render::view_tab_strip_browser_tabs::ViewStripBrowserTab;
use crate::*;

/// One drawn tab of the strip.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewStripEntry {
    View(TitlebarMode),
    Browser(ViewStripBrowserTab),
}

impl ViewStripEntry {
    pub(crate) fn key(self) -> ViewStripTabKey {
        match self {
            Self::View(mode) => ViewStripTabKey::View(mode),
            Self::Browser(tab) => ViewStripTabKey::Browser(tab.tab_id),
        }
    }
}

impl GhostexGpuiApp {
    /// The strip in drawn order: pinned tabs first, then the rest, each group in the order the
    /// user dragged it into. A tab the stored order has not seen yet is drawn at the end.
    pub(crate) fn view_strip_entries(&self) -> Vec<ViewStripEntry> {
        let available = self
            .open_view_tabs()
            .into_iter()
            .filter(|mode| *mode != TitlebarMode::Browser)
            .map(ViewStripEntry::View)
            .chain(
                self.view_strip_browser_tabs()
                    .into_iter()
                    .map(ViewStripEntry::Browser),
            )
            .collect();
        self.view_strip_layout
            .in_drawn_order(available, |entry| entry.key())
    }

    pub(crate) fn view_strip_tab_pinned(&self, key: ViewStripTabKey) -> bool {
        self.view_strip_layout.pinned.contains(&key)
    }

    pub(crate) fn view_strip_tab_position(&self, key: ViewStripTabKey) -> Option<usize> {
        self.view_strip_entries()
            .iter()
            .position(|entry| entry.key() == key)
    }

    pub(crate) fn toggle_view_strip_tab_pinned(
        &mut self,
        key: ViewStripTabKey,
        cx: &mut gpui::Context<Self>,
    ) {
        // Freeze the drawn order first, so the tab keeps its place among its new neighbours rather
        // than jumping to wherever an older stored order had it.
        let drawn = self.view_strip_keys_with_undrawn();
        if self.view_strip_tab_pinned(key) {
            self.view_strip_layout
                .pinned
                .retain(|pinned| *pinned != key);
            // Unpinning puts the tab at the head of the unpinned tabs, right where it was.
            self.view_strip_layout.order = drawn;
        } else {
            // Pinning sends it to the end of the pinned group.
            let mut order = drawn;
            order.retain(|existing| *existing != key);
            let pinned_count = order
                .iter()
                .filter(|existing| self.view_strip_tab_pinned(**existing))
                .count();
            order.insert(pinned_count, key);
            self.view_strip_layout.order = order;
            self.view_strip_layout.pinned.push(key);
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// A tab that just opened goes last. The drawn order is stored first, so every tab already on
    /// screen, including browser tabs the stored order had not seen yet, keeps its place ahead of it.
    pub(crate) fn append_view_strip_tab(&mut self, key: ViewStripTabKey) {
        let mut order = self.view_strip_keys_with_undrawn();
        order.retain(|existing| *existing != key);
        order.push(key);
        self.view_strip_layout.order = order;
    }

    /// The drawn keys, followed by the stored keys that are not drawn right now (a view this
    /// project cannot show at the moment, a page that has not loaded) so they keep a place to come
    /// back to. Keys whose tab is gone for good are dropped here.
    fn view_strip_keys_with_undrawn(&self) -> Vec<ViewStripTabKey> {
        let mut keys = self
            .view_strip_entries()
            .into_iter()
            .map(ViewStripEntry::key)
            .collect::<Vec<_>>();
        for key in &self.view_strip_layout.order {
            let exists = match key {
                ViewStripTabKey::View(mode) => self.open_views.contains(mode),
                ViewStripTabKey::Browser(tab_id) => self.browser_tabs.tab(*tab_id).is_some(),
            };
            if exists && !keys.contains(key) {
                keys.push(*key);
            }
        }
        keys
    }

    pub(crate) fn set_view_strip_drop_index(
        &mut self,
        index: Option<usize>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.view_strip_drop_index != index {
            self.view_strip_drop_index = index;
            cx.notify();
        }
    }

    /// A dragged tab is over the tab drawn at `tab_index`: the marker goes before or after it,
    /// whichever half the pointer is in.
    pub(crate) fn update_view_strip_drop_feedback(
        &mut self,
        bounds: gpui::Bounds<Pixels>,
        position: gpui::Point<Pixels>,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        if !bounds.contains(&position) {
            return;
        }
        let insertion_index = workspace_tab_insertion_index(bounds, position, tab_index);
        self.set_view_strip_drop_index(Some(insertion_index), cx);
    }

    /// CDXC:Workarea 2026-09-22 WHY:
    /// The drop line is drawn in the gap between two tabs, and the strip is taller than its tabs, so a release aimed at the line is often over no tab at all and no tab's drop ran while the line was still showing. The strip itself takes those releases and lands the tab where the line is; a tab under the pointer still answers first. The line goes away once the pointer leaves the strip, so a release elsewhere reorders nothing.
    pub(crate) fn handle_view_strip_gap_drop(
        &mut self,
        key: ViewStripTabKey,
        cx: &mut gpui::Context<Self>,
    ) {
        match self.view_strip_drop_index {
            Some(insertion_index) => self.handle_view_strip_drop(key, insertion_index, cx),
            // Taking the drop keeps the release from the window root, which is what ends a browser tab's drag.
            None => self.finish_browser_tab_drag(cx),
        }
    }

    pub(crate) fn clear_view_strip_drop_outside(
        &mut self,
        bounds: gpui::Bounds<Pixels>,
        position: gpui::Point<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !bounds.contains(&position) {
            self.set_view_strip_drop_index(None, cx);
        }
    }

    pub(crate) fn handle_view_strip_drop(
        &mut self,
        key: ViewStripTabKey,
        default_insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let insertion_index = self
            .view_strip_drop_index
            .take()
            .unwrap_or(default_insertion_index);
        // A browser tab's drag hides the pages so the panes can show their drop zones; a drop on
        // the strip ends that the same way a release anywhere else does.
        self.finish_browser_tab_drag(cx);
        let mut keys = self.view_strip_keys_with_undrawn();
        let drawn_count = self.view_strip_entries().len();
        let Some(from) = keys.iter().position(|existing| *existing == key) else {
            cx.notify();
            return;
        };
        let mut target = insertion_index.min(drawn_count);
        if target > from {
            target -= 1;
        }
        keys.remove(from);
        // Pinned tabs stay together at the left: a drag moves a tab within its own group only.
        let pinned_count = keys
            .iter()
            .take(drawn_count.saturating_sub(1))
            .filter(|existing| self.view_strip_tab_pinned(**existing))
            .count();
        let target = if self.view_strip_tab_pinned(key) {
            target.min(pinned_count)
        } else {
            target.max(pinned_count)
        };
        keys.insert(target.min(keys.len()), key);
        self.view_strip_layout.order = keys;
        self.sync_open_views_to_view_strip_order();
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// The open-views list keeps the views in the order the strip draws them, because closing a tab
    /// picks its successor from that list and the number hotkeys walk it. Views the strip is not
    /// drawing (Browser, a view this project cannot show) keep their slots.
    fn sync_open_views_to_view_strip_order(&mut self) {
        let drawn = self.strip_view_tabs();
        let mut next = drawn.iter().copied();
        for slot in &mut self.open_views {
            if drawn.contains(slot)
                && let Some(mode) = next.next()
            {
                *slot = mode;
            }
        }
    }

    /// A drag released anywhere but on a tab ends here, so the drop marker cannot outlive the drag
    /// that drew it. The window root calls this on every left mouse up, beside the other tab drags.
    pub(crate) fn cancel_view_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_view_strip_drop_index(None, cx);
    }
}
