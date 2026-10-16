//! A counter that moves whenever any workspace's connectors change, so each session's
//! `ghostex connectors mcp` can wait for a change instead of asking for its tools on a timer.
//!
//! CDXC:Connectors 2026-10-17 WHY:
//! Every Claude session runs its own `ghostex connectors mcp`, and with about 25 of them asking for their tools every 15 seconds (and every 5 while gxserver was slow) gxserver spent its time on the tool route and every API call slowed to 15-25 seconds (2026-10-17, reported by the Main Coordinator with a `sample` of gxserver). Each helper now holds one waiting request that gxserver answers when the counter moves (`connectorsChanged`) or after a few minutes, and lists its tools only then.

use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::watch;

fn channel() -> &'static watch::Sender<u64> {
    static REVISION: OnceLock<watch::Sender<u64>> = OnceLock::new();
    REVISION.get_or_init(|| watch::channel(1).0)
}

/// The current revision.
pub(crate) fn tools_revision() -> u64 {
    *channel().borrow()
}

/// Something about a workspace's connectors changed.
pub(crate) fn bump_tools_revision() {
    channel().send_modify(|revision| *revision += 1);
}

/// Waits until the revision is no longer `seen`, at most `limit`, and returns the revision then.
pub(crate) async fn wait_past(seen: u64, limit: Duration) -> u64 {
    let mut receiver = channel().subscribe();
    let _ = tokio::time::timeout(limit, receiver.wait_for(|revision| *revision != seen)).await;
    tools_revision()
}
