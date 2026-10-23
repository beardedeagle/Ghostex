//! A Browser page's own notifications (`new Notification(…)`, a service worker's
//! `showNotification`) as real system notifications once the site is allowed to show them.
//!
//! CDXC:Browser 2026-10-23 WHY:
//! Alloy-style CEF has no notification display, so a site the user allowed to notify (claude.ai, after its "Enable notifications") still showed nothing. Each Browser tab's pages get a small script that keeps the page's own Notification API and also hands every notification shown while permission is granted to the app over the page bridge (cef/shell/page_bridge.rs), which posts it with Ghostex's own notification path; clicking it brings the page back.
//! SEE-ALSO: cef/shell/site_permissions.rs (the Allow or Block that grants the permission).

use crate::app::model::*;
use crate::*;
use futures::StreamExt as _;
use serde_json::Value;

/// What a notification click carries: the page's address after this prefix.
pub(crate) const BROWSER_PAGE_NOTIFICATION_PREFIX: &str = "browser-page:";

/// Wraps the page's Notification API; only shown notifications are reported, only while the
/// site's permission is granted, at most ten a minute.
const PAGE_NOTIFICATION_SCRIPT: &str = r#"(() => {
  const Native = window.Notification;
  if (!Native || Native.__ghostexRelay) return;
  const sent = [];
  const relay = (title, options) => {
    try {
      if (Native.permission !== 'granted') return;
      const now = Date.now();
      while (sent.length && now - sent[0] > 60000) sent.shift();
      if (sent.length >= 10) return;
      sent.push(now);
      window.__ghostexPageBridge(JSON.stringify({
        type: 'notification',
        title: String(title || '').slice(0, 200),
        body: String((options && options.body) || '').slice(0, 500),
        url: location.href,
      }));
    } catch (_) {}
  };
  function GhostexNotification(title, options) {
    relay(title, options);
    return new Native(title, options);
  }
  GhostexNotification.prototype = Native.prototype;
  Object.defineProperty(GhostexNotification, 'permission', { get: () => Native.permission });
  Object.defineProperty(GhostexNotification, 'maxActions', { get: () => Native.maxActions });
  GhostexNotification.requestPermission = (...args) => Native.requestPermission(...args);
  GhostexNotification.__ghostexRelay = true;
  window.Notification = GhostexNotification;
  const registration = window.ServiceWorkerRegistration && ServiceWorkerRegistration.prototype;
  if (registration && registration.showNotification) {
    const show = registration.showNotification;
    registration.showNotification = function (title, options) {
      relay(title, options);
      return show.call(this, title, options).catch(() => {});
    };
  }
})();"#;

impl GhostexGpuiApp {
    /// Starts relaying a Browser tab page's notifications (called when its page is created in
    /// `profile`), for as long as the page lives.
    pub(crate) fn relay_browser_page_notifications(
        &mut self,
        surface: &Entity<CefSurface>,
        profile: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(mut bridge) =
            CefSurface::install_page_bridge(surface, PAGE_NOTIFICATION_SCRIPT, cx)
        else {
            support_logs::append_for_scenario(
                support_logs::GpuiSupportLog::SidebarRefresh,
                "native.sidebar.refresh",
                "gpui.browserPageNotification.noBridge",
                serde_json::json!({}),
            );
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(message) = bridge.messages.next().await {
                let Ok(message) = serde_json::from_str::<Value>(&message) else {
                    continue;
                };
                if message["type"] != "notification" {
                    continue;
                }
                let text = |key: &str| message[key].as_str().unwrap_or_default().trim().to_string();
                let (title, body, url) = (text("title"), text("body"), text("url"));
                if title.is_empty() && body.is_empty() {
                    continue;
                }
                // The page could post one itself: only a site the user allowed notifies.
                let allowed = crate::cef::site_may_notify(&profile, &url);
                support_logs::append_for_scenario(
                    support_logs::GpuiSupportLog::SidebarRefresh,
                    "native.sidebar.refresh",
                    "gpui.browserPageNotification.received",
                    serde_json::json!({ "allowed": allowed }),
                );
                if !allowed {
                    continue;
                }
                // Off the main thread, like a session's notification: delivery waits for macOS.
                let delivered = cx
                    .background_executor()
                    .spawn(async move { deliver_page_notification(title, body, url) })
                    .await;
                support_logs::append_for_scenario(
                    support_logs::GpuiSupportLog::SidebarRefresh,
                    "native.sidebar.refresh",
                    "gpui.browserPageNotification.delivered",
                    serde_json::json!({ "result": format!("{delivered:?}") }),
                );
                if this.upgrade().is_none() {
                    break;
                }
            }
        })
        .detach();
    }

    /// A page notification's click: the page's own tab (or a new one) in the Browser.
    pub(crate) fn open_browser_page_from_notification(
        &mut self,
        url: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_browser_url_from_renderer_command(
            GpuiSidebarOpenBrowserUrlMessage {
                url: url.to_string(),
                reuse: GpuiBrowserRendererOpenReuse::Exact,
                from_quick_header: false,
                project_id: None,
            },
            window,
            cx,
        );
    }
}

fn deliver_page_notification(
    title: String,
    body: String,
    url: String,
) -> GpuiMacOSNotificationDeliveryResult {
    let site = url
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(rest).to_string())
        .unwrap_or_default();
    let (title, body) = match (title.is_empty(), site.is_empty()) {
        (true, _) => (site.clone(), body),
        (false, true) => (title, body),
        (false, false) => (
            title,
            format!("{site} · {body}")
                .trim_end_matches(" · ")
                .to_string(),
        ),
    };
    gpui_deliver_macos_session_attention_notification(GpuiSessionAttentionNotificationCandidate {
        body,
        icon_data_url: None,
        session_id: format!("{BROWSER_PAGE_NOTIFICATION_PREFIX}{url}"),
        title,
    })
}
