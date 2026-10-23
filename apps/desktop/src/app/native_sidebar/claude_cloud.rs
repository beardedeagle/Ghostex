//! A Claude cloud session in the sidebar: the cloud glyph on its row and the two actions of its
//! context menu (Open on claude.ai, Bring to This Computer). The GPUI web build compiles this
//! file too.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User picked "the cloud glyph only" for a cloud session's row: no "Claude" label beside it, the
//! tooltip says where it runs and what it last did. The status line rides the tooltip because the
//! row has no room for it.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User picked "hover-only" for the account a cloud session runs on: a plain cloud icon with no account colour or tint; the account's name shows only in the row's tooltip (and in the session's header).
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/session.rs (the menu rows),
//! apps/desktop/src/app/gx_store/sidebar_snapshot.rs (`claudeCloud` in a row's details),
//! server/src/claude_cloud/ (the sessions and `/api/claudeCloud`).

use std::time::Duration;

use ghostex_gx_core::SessionKey;
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, InteractiveElement, IntoElement, ParentElement, Styled, div};
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::{Value, json};

use super::tooltips::SidebarTooltipSpan;
use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc_with_timeout;
use crate::app::helpers::*;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};

/// Bring to This Computer creates a worktree and a session; gxserver bounds it at two minutes.
const BRING_BACK_TIMEOUT: Duration = Duration::from_secs(150);

/// The row's tooltip: where the session runs and its latest status.
fn cloud_tooltip(cloud: &Value) -> String {
    let text = |key: &str| {
        cloud[key]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let place = match cloud["state"].as_str() {
        Some("draft") => "Claude cloud · waiting for its task".to_owned(),
        Some("starting") => "Starting in Claude's cloud…".to_owned(),
        Some("failed") => format!(
            "Could not start in Claude's cloud: {}",
            text("error").unwrap_or("it failed")
        ),
        Some("moved") => "Moved to this computer".to_owned(),
        _ => match text("branch") {
            Some(branch) => format!("Runs in Claude's cloud · {branch}"),
            None => "Runs in Claude's cloud".to_owned(),
        },
    };
    let mut lines = vec![place];
    if let Some(account) = text("accountName") {
        lines.push(match text("accountEmail") {
            Some(email) => format!("Account: {account} ({email})"),
            None => format!("Account: {account}"),
        });
    }
    if cloud["state"] == "running" {
        lines.extend(text("limitNotice").map(str::to_owned));
        lines.extend(text("statusText").map(str::to_owned));
    }
    lines.join("\n")
}

/// The cloud glyph after a cloud session's title, with where it runs in its tooltip.
pub(super) fn claude_cloud_badge(
    session: &NativeSidebarSession,
    appearance: &SidebarAppearance,
    tooltip_span: SidebarTooltipSpan,
    tooltips: bool,
) -> Option<AnyElement> {
    let cloud = session.details.get("claudeCloud")?;
    let tooltip = cloud_tooltip(cloud);
    let scale = appearance.scale;
    Some(
        div()
            .id(format!(
                "native-session-claude-cloud-{}",
                session.session_id
            ))
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(titlebar_svg_icon(
                "titlebar/cloud.svg",
                12.0 * scale,
                appearance.muted,
            ))
            .when(tooltips, |badge| {
                badge.managed_discrete_tooltip_with_placement(
                    tooltip_span.placement(),
                    appearance.tooltip_delay,
                    move |window, cx| {
                        super::tooltips::sidebar_tooltip(
                            tooltip.clone(),
                            tooltip_span,
                            scale,
                            window,
                            cx,
                        )
                    },
                )
            })
            .into_any_element(),
    )
}

impl GhostexGpuiApp {
    /// The drawn row's `claudeCloud` details.
    fn native_sidebar_claude_cloud(&self, row_id: &str) -> Option<Value> {
        self.native_sidebar
            .snapshot
            .as_ref()?
            .groups
            .iter()
            .flat_map(|group| group.sessions.iter())
            .find(|session| session.session_id == row_id)?
            .details
            .get("claudeCloud")
            .cloned()
    }

    /// Opens a cloud session's claude.ai page in the Browser, in the session's project, reusing
    /// the tab already showing it.
    pub(crate) fn open_claude_cloud_page(
        &mut self,
        session: &SessionKey,
        url: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let project_id = session.project_id.clone();
        self.defer_in_main_window(cx, move |this, window, cx| {
            this.open_browser_url_from_renderer_command(
                GpuiSidebarOpenBrowserUrlMessage {
                    url,
                    reuse: GpuiBrowserRendererOpenReuse::Exact,
                    from_quick_header: false,
                    project_id: Some(project_id),
                },
                window,
                cx,
            );
        });
    }

    /// `{ type: 'claudeCloudSessionAction', sessionId, action }` from a cloud session's menu.
    pub(crate) fn run_native_sidebar_claude_cloud_action(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(row_id) = command["sessionId"].as_str() else {
            return;
        };
        let Some(session) =
            SessionKey::parse_sidebar_session_id(row_id).filter(|key| key.machine.is_local())
        else {
            return;
        };
        let url = self
            .native_sidebar_claude_cloud(row_id)
            .and_then(|cloud| cloud["url"].as_str().map(str::to_owned));
        match command["action"].as_str() {
            Some("open") => {
                if let Some(url) = url {
                    self.open_claude_cloud_page(&session, url, cx);
                }
            }
            Some("bringBack") => self.bring_claude_cloud_session_back(session, cx),
            // The account chooser is a native prompt, which the web build does not have.
            #[cfg(not(target_arch = "wasm32"))]
            Some("copy") => self.defer_in_main_window(cx, move |this, window, cx| {
                this.start_claude_cloud_copy(session, window, cx);
            }),
            _ => {}
        }
    }

    /// Bring to This Computer: gxserver continues the session in a new worktree and parks the
    /// cloud row; the new local session is focused when it exists.
    fn bring_claude_cloud_session_back(
        &mut self,
        session: SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Bringing the session to this computer…",
            "Ghostex makes a new worktree and continues the session there.",
            cx,
        );
        let params = json!({
            "action": "bringBack",
            "projectId": session.project_id,
            "sessionId": session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let answer =
                gx_rpc_with_timeout(None, "/api/claudeCloud", params, BRING_BACK_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| match answer {
                Ok(value) => {
                    let local = value["projectId"].as_str().zip(value["sessionId"].as_str());
                    if let Some((project_id, session_id)) = local {
                        let row_id =
                            SessionKey::local(project_id, session_id).to_sidebar_session_id();
                        this.dispatch_native_sidebar_ui(
                            json!({"type": "selectSession", "sessionId": row_id, "mode": "focus"}),
                            cx,
                        );
                    }
                }
                Err(error) => this.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Could not bring the session to this computer",
                    &error.message,
                    cx,
                ),
            });
        })
        .detach();
    }
}
