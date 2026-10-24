//! Where the header's bottom edge is, in main-window coordinates.

use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use crate::app::consts::*;

/*
CDXC:Titlebar 2026-09-20 WHY:
The deleted titlebar row published its height as the `TITLEBAR_HEIGHT` constant, and nine unrelated
places copied it to mean "the top of the content area": every anchored dropdown, the macOS sidebar
and companion reveal panels, the sidebar divider's hot zone, and the command pane's height ratio.
A header that phases 3 and 4 keep growing cannot be a constant in nine places, so it is measured
once per frame by the header's own prepaint and read back through this one function. Everything that
has to sit below the header reads it; nothing recomputes it.
*/
static WORKAREA_HEADER_BOTTOM_Y: AtomicU32 = AtomicU32::new(WORKAREA_HEADER_HEIGHT.to_bits());

pub(crate) fn record_workarea_header_bottom_y(bottom_y: f32) {
    WORKAREA_HEADER_BOTTOM_Y.store(bottom_y.to_bits(), Ordering::Relaxed);
}

/// The y coordinate of the header's bottom edge inside the main window: the top of everything the
/// header sits above, and the anchor line for every dropdown it hosts.
pub(crate) fn workarea_header_bottom_y() -> f32 {
    f32::from_bits(WORKAREA_HEADER_BOTTOM_Y.load(Ordering::Relaxed))
}
