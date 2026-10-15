//! `connectorsChanged`: tells clients a workspace's connectors changed (signed in or out, apps
//! connected, tools listed), so Settings refetches `/api/readConnectors`.

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

/// Wires the Composio code's change listener to the event, for this gxserver's lifetime.
pub(crate) fn start_connectors(state: Arc<AppState>) {
    let state: Weak<AppState> = Arc::downgrade(&state);
    super::composio::set_changed_listener(Arc::new(move |workspace: &str| {
        if let Some(state) = state.upgrade() {
            broadcast_connectors_changed(&state, workspace);
        }
    }));
}
