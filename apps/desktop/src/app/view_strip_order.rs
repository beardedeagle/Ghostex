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
    /// user dragged it into. A tab the stored order has not seen yet is drawn at the end. While the
    /// Browser's pages have a row of their own (`render/browser_page_tabs.rs`) the strip holds the
    /// Browser tab instead of them, standing where its first page stood until it is dragged.
    pub(crate) fn view_strip_entries(&self) -> Vec<ViewStripEntry> {
        let pages_in_row = self.browser_pages_in_view_row;
        let views = self
            .open_view_tabs()
            .into_iter()
            .filter(|mode| pages_in_row || *mode != TitlebarMode::Browser)
            .map(ViewStripEntry::View);
        let pages = if pages_in_row {
            Vec::new()
        } else {
            self.view_strip_browser_tabs()
        };
        self.in_view_strip_order(
            views
                .chain(pages.into_iter().map(ViewStripEntry::Browser))
                .collect(),
            ViewStripEntry::key,
        )
    }

    /// `available` in the strip's drawn order (`GpuiViewStripLayout::in_drawn_order`), the Browser
    /// tab standing in for its pages while they have a row of their own.
    pub(crate) fn in_view_strip_order<T: Copy>(
        &self,
        available: Vec<T>,
        key: impl Fn(T) -> ViewStripTabKey,
    ) -> Vec<T> {
        self.view_strip_layout
            .in_drawn_order(available, self.browser_pages_in_view_row, |entry| {
                key(*entry)
            })
    }

    pub(crate) fn view_strip_tab_pinned(&self, key: ViewStripTabKey) -> bool {
        self.view_strip_layout.pinned.contains(&key)
    }

    pub(crate) fn view_strip_tab_position(&self, key: ViewStripTabKey) -> Option<usize> {
        self.view_strip_entries()
            .iter()
            .position(|entry| entry.key() == key)
    }

    /// Pinning sends the tab to the end of the pinned tabs, those not drawn right now included;
    /// unpinning puts it at the head of the unpinned tabs. A page in the Browser's own row lands
    /// there within the row, which draws the pages in the same stored order. Moving the tab shifts
    /// the slots of the undrawn tabs between its old and new place by one.
    pub(crate) fn toggle_view_strip_tab_pinned(
        &mut self,
        key: ViewStripTabKey,
        cx: &mut gpui::Context<Self>,
    ) {
        // Store the drawn order first, so the tab moves among its neighbours as they are drawn
        // rather than as an older stored order had them.
        let drawn = self.drawn_view_strip_keys(key);
        self.write_view_strip_projection(&drawn);
        let layout = &mut self.view_strip_layout;
        layout.order.retain(|existing| *existing != key);
        let place = if layout.pinned.contains(&key) {
            layout.pinned.retain(|pinned| *pinned != key);
            layout
                .order
                .iter()
                .position(|existing| !layout.pinned.contains(existing))
                .unwrap_or(layout.order.len())
        } else {
            layout.pinned.push(key);
            layout
                .order
                .iter()
                .rposition(|existing| layout.pinned.contains(existing))
                .map_or(0, |last_pinned| last_pinned + 1)
        };
        layout.order.insert(place, key);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// A tab that just opened goes last. The drawn order is stored first, so every tab already on
    /// screen, including one the stored order had not seen yet, keeps its place ahead of it.
    pub(crate) fn append_view_strip_tab(&mut self, key: ViewStripTabKey) {
        let drawn = self.drawn_view_strip_keys(key);
        self.write_view_strip_projection(&drawn);
        let order = &mut self.view_strip_layout.order;
        order.retain(|existing| *existing != key);
        order.push(key);
    }

    /// The keys of the row `key` is drawn in, in drawn order: the Browser's page row for a page
    /// while the pages have a row of their own, the strip otherwise. Before reading the strip, the
    /// Browser tab gets a stored place of its own where it is drawn.
    fn drawn_view_strip_keys(&mut self, key: ViewStripTabKey) -> Vec<ViewStripTabKey> {
        if self.browser_pages_in_view_row && matches!(key, ViewStripTabKey::Browser(_)) {
            return self
                .browser_page_row()
                .into_iter()
                .map(|page| ViewStripTabKey::Browser(page.tab_id))
                .collect();
        }
        let browser_tab = ViewStripTabKey::View(TitlebarMode::Browser);
        let order = &mut self.view_strip_layout.order;
        if self.browser_pages_in_view_row && !order.contains(&browser_tab) {
            let first_page = order
                .iter()
                .position(|stored| matches!(stored, ViewStripTabKey::Browser(_)))
                .unwrap_or(order.len());
            order.insert(first_page, browser_tab);
        }
        self.view_strip_entries()
            .into_iter()
            .map(ViewStripEntry::key)
            .collect()
    }

    fn pinned_view_strip_tab_count(&self, keys: &[ViewStripTabKey]) -> usize {
        keys.iter()
            .filter(|key| self.view_strip_tab_pinned(**key))
            .count()
    }

    /// `GpuiViewStripLayout::write_projection` for the project on screen.
    fn write_view_strip_projection(&mut self, drawn: &[ViewStripTabKey]) {
        let open_views = &self.open_views;
        let browser_tabs = &self.browser_tabs;
        self.view_strip_layout
            .write_projection(drawn, |key| match key {
                ViewStripTabKey::View(mode) => open_views.contains(mode),
                ViewStripTabKey::Browser(tab_id) => browser_tabs.tab(*tab_id).is_some(),
            });
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
        self.move_view_strip_tab(key, insertion_index, cx);
    }

    /// Moves `key` to `insertion_index` of the row it is drawn in (`drawn_view_strip_keys`).
    /// Pinned tabs stay together at the left: a drag moves a tab within its own group only.
    pub(crate) fn move_view_strip_tab(
        &mut self,
        key: ViewStripTabKey,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        // A browser tab's drag hides the pages so the panes can show their drop zones; a drop on
        // a row ends that the same way a release anywhere else does.
        self.finish_browser_tab_drag(cx);
        let mut keys = self.drawn_view_strip_keys(key);
        if let Some(from) = keys.iter().position(|existing| *existing == key) {
            let mut target = insertion_index.min(keys.len());
            if target > from {
                target -= 1;
            }
            keys.remove(from);
            let pinned_count = self.pinned_view_strip_tab_count(&keys);
            let target = if self.view_strip_tab_pinned(key) {
                target.min(pinned_count)
            } else {
                target.max(pinned_count)
            };
            keys.insert(target, key);
            self.write_view_strip_projection(&keys);
            self.sync_open_views_to_view_strip_order();
            self.persist_shell_layout_state();
        }
        cx.notify();
    }

    /// The open-views list keeps the views in the strip's stored order, pinned first, so what reads
    /// it in list order (the panel toggle's first tab, the saved state) sees them as drawn; closing
    /// a tab reads the drawn order itself (`open_views_in_view_strip_order`). Every open view takes
    /// part, one this project cannot show right now included, so a view that comes back has the
    /// same neighbours in both. Browser takes part while the strip holds its tab (`browserPagesInViewRow`); drawn as
    /// its pages, it keeps its own slot.
    fn sync_open_views_to_view_strip_order(&mut self) {
        let browser_in_strip = self.browser_pages_in_view_row;
        let ranked_slot = |mode: &TitlebarMode| browser_in_strip || *mode != TitlebarMode::Browser;
        let views = self
            .open_views
            .iter()
            .copied()
            .filter(ranked_slot)
            .collect::<Vec<_>>();
        let ranked = self.in_view_strip_order(views, ViewStripTabKey::View);
        let mut next = ranked.into_iter();
        for slot in &mut self.open_views {
            if ranked_slot(slot)
                && let Some(mode) = next.next()
            {
                *slot = mode;
            }
        }
    }

    /// Every open view in the order the strip draws it, those this project cannot show right now
    /// included, which is where `view_tab_successor` looks for the tab beside the one leaving.
    pub(crate) fn open_views_in_view_strip_order(&self) -> Vec<TitlebarMode> {
        let pages_in_row = self.browser_pages_in_view_row;
        let pages = if pages_in_row {
            Vec::new()
        } else {
            self.view_strip_browser_tabs()
                .into_iter()
                .map(|page| page.tab_id)
                .collect()
        };
        self.view_strip_layout
            .open_views_in_drawn_order(&self.open_views, &pages, pages_in_row)
    }

    /// A drag released anywhere but on a tab ends here, so the drop marker cannot outlive the drag
    /// that drew it. The window root calls this on every left mouse up, beside the other tab drags.
    pub(crate) fn cancel_view_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_view_strip_drop_index(None, cx);
        self.set_browser_page_row_drop_index(None, cx);
    }
}
