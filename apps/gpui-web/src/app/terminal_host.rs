//! The session terminal in the work area: the desktop's `TerminalView`, attached to the session's daemon through gxserver.
use ghostex_gx_core::SessionKey;
use gpui::{AppContext as _, Entity, px};

use crate::GhostexGpuiApp;
use crate::terminal_element::{TerminalFontConfig, TerminalView, TerminalViewEvent};
use crate::terminal_model::{TerminalAttachConfig, TerminalModel};

impl GhostexGpuiApp {
    pub(crate) fn ensure_terminal(
        &mut self,
        session: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<TerminalView>> {
        if let Some(view) = self.terminals.get(session) {
            return Some(view.clone());
        }
        let endpoint = self.gx_store.endpoint.clone()?;
        crate::terminal_gpui_engine::register_gpui_terminal_engine_fonts(cx);
        let (sink, events) = TerminalView::event_channel();
        let model = TerminalModel::attach(
            TerminalAttachConfig {
                base_url: endpoint.base_url,
                auth_token: endpoint.auth_token,
                project_id: session.project_id.clone(),
                session_id: session.session_id.clone(),
                // Provisional: the element's first prepaint resizes to the real bounds.
                cols: 80,
                rows: 24,
                max_scrollback: 10_000,
            },
            sink,
        )
        .inspect_err(|error| log::error!("terminal attach failed: {error}"))
        .ok()?;
        let mut font = TerminalFontConfig {
            family: "JetBrainsMono Nerd Font".into(),
            size: px(13.0),
            ..TerminalFontConfig::default()
        };
        // The page has no Ghostty config, so only the Settings switch decides the bold weight.
        let bold_uses_heavier_font = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("terminalBoldUsesHeavierFont")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        font.resolve_bold_weight(None, bold_uses_heavier_font);
        let view = cx
            .new(|cx| TerminalView::from_model(model, events, font, Box::new(|_, _, _, _| {}), cx));
        /*
        CDXC:WebGpui 2026-09-28 WHY:
        Command-click on a terminal link opens it in a new browser tab. The desktop routes `OpenUrlRequested` through its own opener; the page never subscribed, so a link click did nothing, and in Codex's fullscreen view (which captures the mouse) the click used to reach Codex, which opened the URL on the computer running the session instead of the one holding the page.
        */
        cx.subscribe(&view, |_, _, event: &TerminalViewEvent, _| {
            if let TerminalViewEvent::OpenUrlRequested(url) = event
                && (url.starts_with("https://") || url.starts_with("http://"))
                && let Some(window) = web_sys::window()
            {
                let _ = window.open_with_url_and_target(url, "_blank");
            }
        })
        .detach();
        self.terminals.insert(session.clone(), view.clone());
        Some(view)
    }
}
