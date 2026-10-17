//! The element that places the working card (working_card.rs): in the lane the transcript's last
//! row reserves for it, and pinned at that same spot over the transcript's bottom edge once the
//! reader scrolls the lane out of view, so the card never moves.

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, ListState, Pixels, Window, point, px,
};

/// CDXC:SessionChat 2026-10-17 WHY:
/// The card is drawn by the chat over the transcript, not as a row inside it. A row lives in the cached transcript view (transcript_host.rs), and the card's spark animates for as long as the agent works: every frame of it would lay out every visible row again, the cost that view exists to avoid. The last row reserves the card's measured height instead, and this element reads the list's layout from the same frame (it prepaints after the transcript) to set the card on that lane, or keep it at the lane's bottom-of-transcript spot when the lane has scrolled below the view. The card is one element in both places, so it cannot differ in size, look or contents, and it never moves while the reader scrolls up.
pub(super) struct WorkingLane {
    pub(super) list: ListState,
    /// A full-size layer whose content sits at the lane's bottom-of-transcript spot.
    pub(super) child: AnyElement,
}

impl IntoElement for WorkingLane {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for WorkingLane {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // A transcript shorter than the view ends above its bottom: the lane follows its last row
        // there. Otherwise the last row reaches the bottom (or lies below it) and the card stays put.
        let viewport = self.list.viewport_bounds();
        let lift = self
            .list
            .item_count()
            .checked_sub(1)
            .and_then(|last| self.list.bounds_for_item(last))
            .map_or(px(0.0), |row| {
                (row.bottom() - viewport.bottom()).min(px(0.0))
            });
        window.with_element_offset(point(px(0.0), lift), |window| {
            self.child.prepaint(window, cx)
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
