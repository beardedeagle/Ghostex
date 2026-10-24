//! A session's links: opening one where Settings says, removing one, and linking a suggested one.
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/link_chips.rs and
//! apps/desktop/src/app/window/session_links_panel.rs (what sends these),
//! server/src/server/route_http/session_links.rs (`/api/addSessionLink`, `/api/removeSessionLink`).

use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};

impl GhostexGpuiApp {
    /// Answers `openSessionLink {url}`, `removeSessionLink {sessionId, key}` and
    /// `addSessionLink {sessionId, link}`; the session is named by its sidebar id.
    pub(crate) fn gx_store_run_session_links(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match command.get("type").and_then(Value::as_str) {
            Some("openSessionLink") => {
                if let Some(url) = command
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                {
                    self.open_session_link(url.to_string(), cx);
                }
                true
            }
            Some(kind @ ("removeSessionLink" | "addSessionLink")) => {
                let Some(session) = command
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .and_then(SessionKey::parse_sidebar_session_id)
                    .filter(|session| session.machine.is_local())
                else {
                    return true;
                };
                let (path, params, failure) = if kind == "removeSessionLink" {
                    (
                        "/api/removeSessionLink",
                        json!({
                            "projectId": session.project_id,
                            "sessionId": session.session_id,
                            "key": command.get("key").cloned().unwrap_or(Value::Null),
                        }),
                        "Couldn't remove the link",
                    )
                } else {
                    (
                        "/api/addSessionLink",
                        json!({
                            "projectId": session.project_id,
                            "sessionId": session.session_id,
                            "link": command.get("link").cloned().unwrap_or(Value::Null),
                            "source": "user",
                        }),
                        "Couldn't link it",
                    )
                };
                cx.spawn(async move |this, cx| {
                    if let Err(error) = gx_rpc(None, path, params).await {
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_gpui_workspace_action_toast(
                                "error",
                                failure,
                                &error.message,
                                cx,
                            );
                        });
                    }
                })
                .detach();
                true
            }
            _ => false,
        }
    }

    /// Opens a PR, issue or Linear page in Ghostex's Browser, or in the default browser when
    /// Settings > General > Browser > Open session links in says so.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "user can pick to open in embedded or external browser in settings (default to embedded)" (`sessionLinkOpenTarget`: `embedded` or `external`). A Work mode card's own chips keep opening the ticket in the Work view (CDXC:WorkMode 2026-10-09).
    pub(crate) fn open_session_link(&mut self, url: String, cx: &mut gpui::Context<Self>) {
        let external = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("sessionLinkOpenTarget")
            .and_then(Value::as_str)
            == Some("external");
        if external {
            cx.open_url(&url);
            return;
        }
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
    }
}
