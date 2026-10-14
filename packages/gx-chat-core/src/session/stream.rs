//! Sequencing: which frames apply, which are duplicates, and which mean a gap.
//!
//! Ported from `acceptSequencedFrame` in `packages/shared/session-chat-controller/controller.ts`
//! and the ordering rules of `apps/desktop/sidebar/session-chat-runtime/store.ts`.

use crate::state::FramePosition;

/// What to do with a sequenced frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// In order: fold it.
    Apply,
    /// At or behind the accepted position: a duplicate, and dropping it is correct.
    Drop,
    /// More than one ahead, or a different epoch: the stream has a hole only a read can fill.
    Resync,
}

/// Decides one `sessionChatAppended` or `sessionChatState` frame, advancing the cursor when it
/// applies.
pub fn accept_sequenced_frame(position: &mut FramePosition, epoch: i64, seq: i64) -> Verdict {
    if position.epoch == Some(epoch) {
        if seq <= position.seq {
            return Verdict::Drop;
        }
        if seq == position.seq + 1 {
            position.seq = seq;
            return Verdict::Apply;
        }
    }
    Verdict::Resync
}

/// A snapshot or replaced frame is authoritative: it sets the cursor rather than being checked
/// against it, and it latches "a frame has arrived" for the initial-window watchdog.
pub fn accept_authoritative_frame(
    position: &mut FramePosition,
    server_id: &str,
    epoch: i64,
    seq: i64,
) {
    position.server_id = server_id.to_string();
    position.epoch = Some(epoch);
    position.seq = seq;
    position.frame_arrived = true;
}

/// Whether a snapshot the store already holds outranks the one that just arrived.
///
/// Only within one `server_id`: a different one restarts the numbering, so it is never behind.
/// gxserver keeps its `server_id` across restarts and hands a restarted follower a higher epoch.
pub fn snapshot_is_stale(
    position: &FramePosition,
    confirmed: bool,
    server_id: &str,
    epoch: i64,
    seq: i64,
) -> bool {
    confirmed
        && position.server_id == server_id
        && position
            .epoch
            .is_some_and(|current| epoch < current || (epoch == current && seq < position.seq))
}
