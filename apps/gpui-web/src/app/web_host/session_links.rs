//! A session's links list in the browser: the desktop opens it as a header dropdown (a native child
//! window, apps/desktop/src/app/window/session_links_panel.rs), which a page does not have, so a
//! hovered card's "+N" names the links in a toast instead.

use ghostex_gx_core::SessionKey;
use gpui::{Bounds, Context, Pixels, Window};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn open_session_links_list(
        &mut self,
        key: SessionKey,
        _anchor: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sidebar_id = key.to_sidebar_session_id();
        let links = self.native_sidebar.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .groups
                .iter()
                .flat_map(|group| group.sessions.iter())
                .find(|session| session.session_id == sidebar_id)
                .and_then(|session| session.links.clone())
        });
        let Some(links) = links else {
            return;
        };
        let body = links
            .items
            .iter()
            .map(|link| link.summary())
            .collect::<Vec<_>>()
            .join("\n");
        self.dispatch_gpui_workspace_action_toast(
            "info",
            &format!("{} links", links.items.len()),
            &body,
            cx,
        );
    }
}
