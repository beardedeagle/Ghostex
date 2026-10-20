//! The chat view's direct gxserver calls: same signature as the desktop's file, performed with `fetch`.
//!
//! CDXC:WebGpui 2026-09-22 WHY: callers run this inside `background_executor().spawn`, which demands a `Send` future even on the one-threaded web executor, and a JavaScript promise is not `Send`. The fetch therefore runs as a page-local task and only its result, plain JSON, crosses back through a channel. A blocking XMLHttpRequest was tried first and froze the page: the transcript pages older turns in until the viewport is full, and with no repaint between calls it never saw itself fill.
use crate::app::model::*;
use futures::channel::oneshot;
use serde_json::{Value, json};

pub(super) async fn request(
    _remote: Option<GpuiRemoteGxserverRequestTarget>,
    endpoint: &str,
    params: &Value,
) -> Result<Value, Value> {
    let (sender, receiver) = oneshot::channel();
    let path = endpoint.to_string();
    let params = params.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let result = match crate::app::gx_store::web_transport::current_endpoint() {
            Some(target) => {
                crate::app::gx_store::web_transport::rpc_envelope(&target, &path, params).await
            }
            None => Err(json!({"message": "Ghostex is not connected yet.", "endpoint": path})),
        };
        let _ = sender.send(result);
    });
    receiver.await.unwrap_or_else(|_| {
        Err(json!({"message": "The request was dropped.", "endpoint": endpoint}))
    })
}
