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

/// Wires the Composio and Executor change listeners to the event, stops idle Executors, and stops
/// every workspace's Executor when gxserver shuts down, for this gxserver's lifetime.
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
    super::executor::start_idle_stop();
    tokio::spawn(async move {
        let _ = shutdown.recv().await;
        let _ = tokio::task::spawn_blocking(super::executor::stop_all).await;
    });
}
