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
        let mut available = self
            .open_view_tabs()
            .into_iter()
            .filter(|mode| *mode != TitlebarMode::Browser)
            .map(ViewStripEntry::View)
            .chain(
                self.view_strip_browser_tabs()
                    .into_iter()
                    .map(ViewStripEntry::Browser),
            )
            .collect::<Vec<_>>();
        let mut ordered = Vec::with_capacity(available.len());
        for key in &self.view_strip_layout.order {
            if let Some(position) = available.iter().position(|entry| entry.key() == *key) {
                ordered.push(available.remove(position));
            }
        }
        ordered.append(&mut available);
        let (pinned, unpinned): (Vec<_>, Vec<_>) = ordered
            .into_iter()
            .partition(|entry| self.view_strip_tab_pinned(entry.key()));
        pinned.into_iter().chain(unpinned).collect()
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
    /// unpinning puts it at the head of the unpinned tabs. Moving the tab shifts the slots of the
    /// undrawn tabs between its old and new place by one.
    pub(crate) fn toggle_view_strip_tab_pinned(
        &mut self,
        key: ViewStripTabKey,
        cx: &mut gpui::Context<Self>,
    ) {
        // Store the drawn order first, so the tab moves among its neighbours as they are drawn
        // rather than as an older stored order had them.
        self.write_view_strip_order(&self.drawn_view_strip_keys());
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
    /// screen, including browser tabs the stored order had not seen yet, keeps its place ahead of it.
    pub(crate) fn append_view_strip_tab(&mut self, key: ViewStripTabKey) {
        self.write_view_strip_order(&self.drawn_view_strip_keys());
        let order = &mut self.view_strip_layout.order;
        order.retain(|existing| *existing != key);
        order.push(key);
    }

    fn drawn_view_strip_keys(&self) -> Vec<ViewStripTabKey> {
        self.view_strip_entries()
            .into_iter()
            .map(ViewStripEntry::key)
            .collect()
    }

    /// Stores `drawn`, the strip's tabs in their new drawn order, around the tabs it does not draw
    /// right now (a view hidden in this project or one this project cannot show, a page that has
    /// not loaded): each stored slot that holds a drawn tab takes the next of `drawn`, every other
    /// tab keeps its slot, and a drawn tab the order had not placed yet goes last. Keys whose tab
    /// is gone for good are dropped here.
    ///
    /// CDXC:Workarea 2026-10-25 WHY:
    /// The order used to be stored as the drawn tabs followed by the ones not drawn, so any drag, pin or new tab while a view was hidden here sent that view to the end of the strip when it came back. Filling only the drawn tabs' slots keeps the place `open_view_tabs` promises it.
    fn write_view_strip_order(&mut self, drawn: &[ViewStripTabKey]) {
        let mut order = std::mem::take(&mut self.view_strip_layout.order);
        order.retain(|key| match key {
            ViewStripTabKey::View(mode) => self.open_views.contains(mode),
            ViewStripTabKey::Browser(tab_id) => self.browser_tabs.tab(*tab_id).is_some(),
        });
        let mut next = drawn.iter().copied();
        for slot in &mut order {
            if drawn.contains(slot)
                && let Some(key) = next.next()
            {
                *slot = key;
            }
        }
        order.extend(next);
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
        // A browser tab's drag hides the pages so the panes can show their drop zones; a drop on
        // the strip ends that the same way a release anywhere else does.
        self.finish_browser_tab_drag(cx);
        let mut keys = self.drawn_view_strip_keys();
        let Some(from) = keys.iter().position(|existing| *existing == key) else {
            cx.notify();
            return;
        };
        let mut target = insertion_index.min(keys.len());
        if target > from {
            target -= 1;
        }
        keys.remove(from);
        // Pinned tabs stay together at the left: a drag moves a tab within its own group only.
        let pinned_count = keys
            .iter()
            .filter(|existing| self.view_strip_tab_pinned(**existing))
            .count();
        let target = if self.view_strip_tab_pinned(key) {
            target.min(pinned_count)
        } else {
            target.max(pinned_count)
        };
        keys.insert(target, key);
        self.write_view_strip_order(&keys);
        self.sync_open_views_to_view_strip_order();
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// The open-views list keeps the views in the strip's stored order, pinned first, because
    /// closing a tab picks its successor from that list. Every open view takes part, one this
    /// project cannot show right now included, so a view that comes back has the same neighbours in
    /// both; Browser, drawn as its pages, keeps its own slot.
    fn sync_open_views_to_view_strip_order(&mut self) {
        let order = &self.view_strip_layout.order;
        let mut ranked = self
            .open_views
            .iter()
            .copied()
            .filter(|mode| *mode != TitlebarMode::Browser)
            .map(|mode| {
                let key = ViewStripTabKey::View(mode);
                let place = order.iter().position(|stored| *stored == key);
                (
                    (
                        !self.view_strip_tab_pinned(key),
                        place.unwrap_or(usize::MAX),
                    ),
                    mode,
                )
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(rank, _)| *rank);
        let mut next = ranked.into_iter().map(|(_, mode)| mode);
        for slot in &mut self.open_views {
            if *slot != TitlebarMode::Browser
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
