//! `connectorsChanged`: tells clients a workspace's connectors changed (signed in or out, apps
//! connected, tools listed, Executor installed, started or stopped), so Settings refetches
//! `/api/readConnectors`.

use std::sync::{Arc, Weak};

use serde_json::json;

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::server::AppState;

pub(crate) const CONNECTORS_CHANGED_EVENT_TYPE: &str = "connectorsChanged";

pub(crate) fn broadcast_connectors_changed(state: &AppState, workspace: &str) {
    state.event_hub.broadcast(json!({
        "type": CONNECTORS_CHANGED_EVENT_TYPE,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "workspaceId": workspace,
    }));
}

/// Wires the Composio and Executor change listeners to the event and stops every workspace's
/// Executor when gxserver shuts down, for this gxserver's lifetime. The idle-stop thread starts with
/// the first Connectors route served ([`start_connectors_work`]), so it never runs while Connectors
/// is off.
pub(crate) fn start_connectors(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    let weak: Weak<AppState> = Arc::downgrade(&state);
    let listener: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |workspace: &str| {
        if let Some(state) = weak.upgrade() {
            broadcast_connectors_changed(&state, workspace);
        }
    });
    super::composio::set_changed_listener(listener.clone());
    super::executor::set_changed_listener(listener);
    tokio::spawn(async move {
        let _ = shutdown.recv().await;
        let _ = tokio::task::spawn_blocking(super::executor::stop_all).await;
    });
}

/// Starts the background work Connectors needs once it is in use (only Executors started by a
/// route can be idle); safe to call on every route.
pub(crate) fn start_connectors_work() {
    super::executor::start_idle_stop();
}
