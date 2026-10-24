//! The drained envelope: what `nativeChat.take(lastRevision)` returns, built from the Rust core.
//!
//! `apps/desktop/src/app/native_chat/state.rs`'s `apply_output` destructures this object key by key
//! (`minimap`, `rowDetails`, `snapshot`, `requests`) and 24,000 lines of drawing code read the
//! snapshot by string index. `Frame` already carries the TypeScript spelling of every field, so the
//! envelope is one `to_value`; the two transcript splices travel typed beside it. What this file
//! adds is the effects the view performs and the change gate the drain is posted behind.

use ghostex_gx_chat_core::{Frame, HostRequest, ItemsSplice};
use serde_json::Value;

/// One drain, as its view receives it.
pub(crate) struct DrainedFrame {
    /// The object the view applies, without `itemsSplice` and `subagentSplice`.
    pub(crate) envelope: Value,
    /// The changed window of the main transcript list.
    pub(crate) items_splice: Option<ItemsSplice>,
    /// The changed window of the open subagent transcript.
    pub(crate) subagent_splice: Option<ItemsSplice>,
}

/// The object the view applies.
///
/// CDXC:SessionChat 2026-10-24 WHY:
/// Rows reach the view typed and shared with the engine, instead of as a JSON copy each view kept.
/// `to_value` deep-copied every row of both splices, and the view held those copies for its whole
/// life: one more full copy of the transcript per open chat, 65 MB on a 3,400-message chat.
/// A row's models are the `Arc`s the core's projection cache holds, so a splice costs next to
/// nothing to send to the view's thread.
pub(super) fn envelope(mut frame: Frame, requests: Vec<HostRequest>) -> DrainedFrame {
    frame.requests = requests;
    let items_splice = frame.items_splice.take();
    let subagent_splice = frame.subagent_splice.take();
    DrainedFrame {
        envelope: serde_json::to_value(&frame).unwrap_or(Value::Null),
        items_splice,
        subagent_splice,
    }
}

/// Whether a drain is worth posting.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// All SIX channels, not the four `runtime_worker.rs` tests. `take` has already advanced its sent
/// pointers by the time the gate runs, so a drain whose only change is the minimap or the subagent
/// splice was dropped and never re-sent (`docs/2026-09-21/rust-chat/SEAM.md` section 5.1): the rail
/// stopped updating and an open subagent transcript froze until something else changed.
pub(super) fn envelope_carries_change(drained: &DrainedFrame) -> bool {
    drained.items_splice.is_some()
        || drained.subagent_splice.is_some()
        || drained.envelope.as_object().is_some_and(|frame| {
            frame.contains_key("minimap")
                || frame.contains_key("rowDetails")
                || frame.contains_key("snapshot")
                || frame
                    .get("requests")
                    .and_then(Value::as_array)
                    .is_some_and(|requests| !requests.is_empty())
        })
}
