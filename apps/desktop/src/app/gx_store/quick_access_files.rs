//! Quick Access's Files tab asks one machine at a time: this computer's gxserver for its own
//! projects, a remote machine's for its projects, each listing and ranking its own files
//! (`/api/searchProjectFiles`). The GPUI web build compiles this file too.
//!
//! SEE-ALSO: packages/gx-core/src/quick_access/files.rs (the tab), server/src/project_file_search.rs.

use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;

impl GhostexGpuiApp {
    /// Answers a `requestProjectFiles` with a `projectFilesResult` for the same machine and
    /// request. A machine that cannot be reached answers with no files.
    pub(crate) fn gx_store_answer_project_files(
        &mut self,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_id = command
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let machine_id = command
            .get("machineId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let target = machine_id
            .as_deref()
            .map(|machine_id| self.gpui_remote_gxserver_request_target(machine_id));
        let params = json!({
            "limit": command.get("limit").cloned().unwrap_or(Value::Null),
            "projectIds": command.get("projectIds").cloned().unwrap_or_else(|| json!([])),
            "query": command.get("query").cloned().unwrap_or_else(|| json!("")),
        });
        cx.spawn(async move |this, cx| {
            let projects = match target {
                // A remote machine that is not connected.
                Some(None) => None,
                Some(remote) => gx_rpc(remote, "/api/searchProjectFiles", params).await.ok(),
                None => gx_rpc(None, "/api/searchProjectFiles", params).await.ok(),
            }
            .and_then(|result| result.get("projects").cloned())
            .unwrap_or_else(|| json!([]));
            let mut payload = json!({
                "projects": projects,
                "requestId": request_id,
                "type": "projectFilesResult",
            });
            if let Some(machine_id) = machine_id {
                payload["machineId"] = json!(machine_id);
            }
            let _ = this.update(cx, |app, cx| app.quick_access_receive(payload, cx));
        })
        .detach();
    }
}
