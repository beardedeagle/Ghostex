//! Issues Workflow's sidebar commands: the briefcase that opens the Issues view, and a session
//! card's work chip.
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/work_chips.rs (the chips), server/src/work_mode/
//! (the links the chips show).

use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};

impl GhostexGpuiApp {
    /// Answers `openWorkLink` (a chip's click, at the top level) and `openWorkView` (the sidebar's
    /// briefcase).
    pub(crate) fn gx_store_run_work_mode(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match command.get("type").and_then(Value::as_str) {
            Some("openWorkView") => {
                self.defer_in_main_window(cx, |this, window, cx| {
                    this.toggle_work_view(window, cx);
                });
                true
            }
            // CDXC:WorkMode 2026-10-09 DECISION:
            // User: clicking a chip (a PR, an ID) on a session card opens that ticket's details in the Work view in the side panel; the chip's link opens instead where the Work view cannot show.
            Some("openWorkLink")
                if command.get("workItem").is_some_and(Value::is_object)
                    && self.open_work_item(command["workItem"].clone(), cx) =>
            {
                true
            }
            Some("openWorkLink") => {
                let Some(url) = command
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    .map(str::to_string)
                else {
                    return true;
                };
                let message = GpuiSidebarOpenBrowserUrlMessage {
                    url,
                    reuse: GpuiBrowserRendererOpenReuse::Exact,
                    from_quick_header: false,
                    project_id: None,
                };
                self.defer_in_main_window(cx, move |this, window, cx| {
                    if !this.project_switch_pending_requests.is_empty() {
                        this.flush_coalesced_project_switch_requests(window, cx);
                    }
                    this.open_browser_url_from_renderer_command(message, window, cx);
                });
                true
            }
            _ => false,
        }
    }
}
