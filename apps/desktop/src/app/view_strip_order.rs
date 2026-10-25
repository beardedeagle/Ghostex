//! The view panel's strip as one ordered row: which tab is drawn where, which are pinned, and what
//! a drag along the row does.

use crate::app::model::*;
use crate::app::render::view_tab_strip_browser_tabs::ViewStripBrowserTab;
use crate::*;

/// The most tabs `in_view_strip_order` places by scanning the stored order; see its WHY.
const RANKED_VIEW_STRIP_TABS: usize = 32;

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

    /// `available` in drawn order: pinned tabs first, then by place in the one stored order the
    /// strip and the Browser's page row share, a tab it does not name last; ties keep `available`'s
    /// own order.
    ///
    /// CDXC:Workarea 2026-10-25 WHY:
    /// Up to `RANKED_VIEW_STRIP_TABS` tabs each tab's place is a scan of the stored order, then one stable sort; past that the stored order takes its tabs out of the list one by one (the 2026-09-21 loop). Measured, best of 7 in a release build, ns per call for ordered/shuffled tabs with 2 pins: 4 tabs 49/49 against the loop's 95/91, 32 tabs 454/517 against 462/578, 40 tabs 594/809 against 610/748, 64 tabs 1,290/1,636 against 921/1,303, 128 tabs 4,771/5,264 against 2,427/3,470. A `HashMap` of places (std's SipHash, the only map this crate uses) was slower than the loop at every size up to 128, and finding places on a shrinking `swap_remove` list was slower than the scan up to 16 tabs and than the loop from 64.
    pub(crate) fn in_view_strip_order<T: Copy>(
        &self,
        available: Vec<T>,
        key: impl Fn(T) -> ViewStripTabKey,
    ) -> Vec<T> {
        let order = &self.view_strip_layout.order;
        let browser_tab = ViewStripTabKey::View(TitlebarMode::Browser);
        let browser_tab_unplaced = self.browser_pages_in_view_row && !order.contains(&browser_tab);
        let names = |stored: &ViewStripTabKey, key: ViewStripTabKey| {
            *stored == key
                || browser_tab_unplaced
                    && key == browser_tab
                    && matches!(stored, ViewStripTabKey::Browser(_))
        };
        if available.len() > RANKED_VIEW_STRIP_TABS {
            let mut remaining = available;
            let mut ordered = Vec::with_capacity(remaining.len());
            for stored in order {
                if let Some(index) = remaining
                    .iter()
                    .position(|entry| names(stored, key(*entry)))
                {
                    ordered.push(remaining.remove(index));
                }
            }
            ordered.append(&mut remaining);
            let (pinned, unpinned): (Vec<_>, Vec<_>) = ordered
                .into_iter()
                .partition(|entry| self.view_strip_tab_pinned(key(*entry)));
            return pinned.into_iter().chain(unpinned).collect();
        }
        let mut placed = available
            .into_iter()
            .map(|entry| {
                let key = key(entry);
                let place = order.iter().position(|stored| names(stored, key));
                (
                    !self.view_strip_tab_pinned(key),
                    place.unwrap_or(usize::MAX),
                    entry,
                )
            })
            .collect::<Vec<_>>();
        placed.sort_by_key(|(unpinned, place, _)| (*unpinned, *place));
        placed.into_iter().map(|(_, _, entry)| entry).collect()
    }

    pub(crate) fn view_strip_tab_pinned(&self, key: ViewStripTabKey) -> bool {
        self.view_strip_layout.pinned.contains(&key)
    }

    pub(crate) fn view_strip_tab_position(&self, key: ViewStripTabKey) -> Option<usize> {
        self.view_strip_entries()
            .iter()
            .position(|entry| entry.key() == key)
    }

    /// Pinning sends the tab to the end of the pinned tabs; unpinning leaves it where it is drawn,
    /// which is the head of the unpinned ones. A page in the Browser's own row moves within the row.
    /// With the setting off the pin works on the whole order, the drawn tabs followed by the ones
    /// not drawn right now, as the strip always has, so a pinned view hidden in this project still
    /// counts among the pinned tabs a new pin goes after.
    pub(crate) fn toggle_view_strip_tab_pinned(
        &mut self,
        key: ViewStripTabKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let whole_order = !self.browser_pages_in_view_row;
        let mut keys = self.drawn_view_strip_keys(key);
        if whole_order {
            self.write_view_strip_projection(&keys);
            keys = std::mem::take(&mut self.view_strip_layout.order);
        }
        if self.view_strip_tab_pinned(key) {
            self.view_strip_layout
                .pinned
                .retain(|pinned| *pinned != key);
        } else {
            keys.retain(|existing| *existing != key);
            let pinned_count = self.pinned_view_strip_tab_count(&keys);
            keys.insert(pinned_count, key);
            self.view_strip_layout.pinned.push(key);
        }
        if whole_order {
            self.view_strip_layout.order = keys;
        } else {
            self.write_view_strip_projection(&keys);
        }
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

    /// Writes a reordered row (the strip, or the Browser's page row) back into the one stored
    /// order, after dropping keys whose tab is gone for good. With the setting off the order
    /// becomes `drawn` followed by the stored keys it does not draw, as the strip has always
    /// written it, so the strip stays exactly as it was. With it on, the strip and the page row
    /// each draw only part of the order, so each stored slot that holds one of `drawn`'s keys takes
    /// the next of them and every other tab keeps its slot; keys the order had not placed yet go
    /// last.
    fn write_view_strip_projection(&mut self, drawn: &[ViewStripTabKey]) {
        let mut order = std::mem::take(&mut self.view_strip_layout.order);
        order.retain(|key| match key {
            ViewStripTabKey::View(mode) => self.open_views.contains(mode),
            ViewStripTabKey::Browser(tab_id) => self.browser_tabs.tab(*tab_id).is_some(),
        });
        if self.browser_pages_in_view_row {
            let mut next = drawn.iter().copied();
            for slot in &mut order {
                if drawn.contains(slot)
                    && let Some(key) = next.next()
                {
                    *slot = key;
                }
            }
            order.extend(next);
        } else {
            order.retain(|key| !drawn.contains(key));
            order.splice(0..0, drawn.iter().copied());
        }
        self.view_strip_layout.order = order;
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
        self.set_browser_page_row_drop_index(None, cx);
    }
}
