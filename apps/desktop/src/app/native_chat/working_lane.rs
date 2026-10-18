//! The element that places the working card (working_card.rs): in its own row at the top of the
//! composer's stack, or, under a transcript too short to reach that row, right after its last row.

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, ListState, Pixels, Window, point, px,
};

/// CDXC:SessionChat 2026-10-18 WHY:
/// The card is a row of the composer's stack, not a transcript row: a row lives in the cached transcript view (transcript_host.rs), and the card's spark animates for as long as the agent works, so every frame of it would lay out every visible row again, the cost that view exists to avoid. Being outside the list, the transcript's visible area ends above it and nothing ever scrolls under it (supersedes the 2026-10-17 card that the last row reserved room for and that stayed pinned over the transcript's bottom edge). A transcript shorter than the view would leave the card far below its last row, so this element reads the list's layout from the same frame (the composer prepaints after the transcript) and lifts the card into the list's empty end, right after the last row; the lift shrinks to nothing as the rows reach the view's bottom, so the card is one element in one place that never jumps.
pub(super) struct WorkingLane {
    pub(super) list: ListState,
    /// The card, laid out in its row.
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
        // A transcript shorter than the view ends above its bottom: the card follows its last row
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
