//! The transcript's own scrollbar.
//!
//! gpui-component's `Scrollbar` already implements `ScrollbarHandle` for gpui's
//! `ListState`, so the chat list needs no hand-rolled thumb: the component reads
//! the list's offset and content size, drags it through
//! `set_offset_from_scrollbar`, and brackets the drag with the list's own
//! `scrollbar_drag_started` / `scrollbar_drag_ended`.
//!
//! Its mouse handling is scoped to the bar's strip (the component's 16px track,
//! only while the bar shows), so it never takes a click, a drag or a selection
//! away from the transcript underneath it. Every composer control is inset by
//! the footer's own 16px padding, and the minimap's dashes end 15.6px from the
//! pane's edge, so the column beside them covers nothing interactive.

use super::state::NativeChatView;
use crate::app::window::native_modal_kit::app_vertical_scrollbar;
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, IntoElement as _, ParentElement as _, Styled as _, div, px, size};
use gpui_component::scroll::Scrollbar;

/// Unscaled track and thumb width of the transcript's table scrollbars, the value in
/// session-chat-scrollbar.css. The transcript's own scrollbar does not use it (see below).
pub(super) const THICKNESS: f32 = 5.0;

impl NativeChatView {
    /// The thin thumb beside the transcript: it fades in while the list scrolls
    /// or the pointer is on it, fades out when the reader stops, and drags.
    ///
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// React's track was `top: 0; bottom: 0` of the message scroller, and the scroller spanned the whole
    /// chat pane because the composer was an overlay over it and the transcript kept its end clear
    /// with a bottom inset that tracked the box (use-session-chat-composer-inset.ts), so the viewport
    /// and the track kept their length whatever the box did. The native composer is a real sibling
    /// region below the list, so the list's viewport is what shrinks as the box grows, and a track
    /// bound to that viewport ended higher up the pane on every grow, collapse and expand: the thumb
    /// jumped. So the track is a column of its own reaching the bottom of the pane, measured once per
    /// frame, and the scroll size reported to the component is `track + the list's max offset`, which
    /// keeps `content - track` equal to the list's scrollable distance exactly. That was React's math
    /// (a constant viewport against a content height that carries the composer inset), and it leaves
    /// the thumb still while the box resizes.
    ///
    /// CDXC:SessionChat 2026-09-22 WHY:
    /// That scroll size is handed to the component as a closure, read when the bar lays out, not
    /// computed here while the pane builds its elements. Built here it was the previous frame's max
    /// offset: a streaming reply is spliced out and remeasured every frame, so the list's content
    /// height at build time lacked the row being remeasured while the offset the bar read after
    /// the list's layout included it, and the thumb jumped down and back on every streamed line
    /// while the reader sat at the end of the transcript.
    ///
    /// CDXC:SessionChat 2026-09-21 DECISION:
    /// "We should only show it in the gpui chat view when the user scrolls", not when new messages
    /// appear. The component's own rule shows the bar on any offset change, and the offset of a list
    /// following its tail moves with every streamed line, so the bar is shown from the list's scroll
    /// handler instead, which gpui calls for the reader's wheel scrolls only.
    ///
    /// CDXC:SessionChat 2026-09-28 DECISION:
    /// "Make the scrollbar stick to the side in the chat view when it's wide, like we do when it's narrow." The bar draws in this column (`viewport_from_layout`), not in the list's viewport, which gpui-component uses by default: on a wide pane the list sits between the minimap's gutters, so the bar floated left of the minimap rail instead of on the pane's right edge, and it ended at the list's bottom rather than the pane's.
    ///
    /// CDXC:SessionChat 2026-10-11 DECISION:
    /// User: "make the scrollbar in the chat view EXACTLY match the scroll bar in the files view when looking at md file (gap from edge, how it gets bigger when i hover, the width values color etc". Supersedes the 5px thumb from React's session-chat-scrollbar.css. The bar keeps gpui-component's own geometry and the app theme's thumb, as the Files view's Markdown scrollbar does (`render_body_scrollbar` in native_docs/markdown_body.rs): a 16px strip, a 6px thumb 4px from the edge that grows to 8px over 300ms when the pointer is on it or drags it, the 2px theme radius, a 48px minimum length, and the same colour at rest, on hover and while dragging. Setting `thickness` here, or scaling with the chat zoom, would part the two again. Only when it shows still differs: the Files bar is always shown, this one only while the reader scrolls (the 2026-09-21 decision above).
    pub(super) fn transcript_scrollbar(&self) -> AnyElement {
        let track = self.scrollbar_track.get();
        let measured = track > px(0.0);
        let mut bar = app_vertical_scrollbar(&self.list)
            .id("chat-transcript-scrollbar")
            .viewport_from_layout()
            .shown_by_host_scroll(self.transcript_scrolled_at);
        if measured {
            let list = self.list.clone();
            bar = bar
                .scroll_size_with(move || size(px(0.0), track + list.max_offset_for_scrollbar().y));
        }
        let next_track = self.scrollbar_track.clone();
        let pane = self.bounds.clone();
        div()
            .absolute()
            .top_0()
            .right_0()
            .w(Scrollbar::width())
            // Before the first measurement the column is the transcript region alone, the placement
            // and the content size the component computes on its own.
            .map(|column| {
                if measured {
                    column.h(track)
                } else {
                    column.bottom_0()
                }
            })
            .child(bar)
            .child(
                // Visual-only and non-interactive: it reads where this column starts, which with the
                // pane's own rect gives the distance to the pane's bottom edge.
                gpui::canvas(
                    move |rect, window, cx| {
                        let height = pane.get().bottom() - rect.origin.y;
                        if height > px(0.0) && next_track.replace(height) != height {
                            cx.notify(window.current_view());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}
