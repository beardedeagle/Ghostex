//! The work area header's link chips: after the session's title, its PRs, issues and Linear
//! tickets, then "+N" (or a list button) that opens the whole list.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User, of the chips in the work area header: "yes yes yes love this". Up to three chips (one when the header is compact) follow the title, open work first; the title gives way before them. The chips show for every session, not only in Work mode (the user's "everywhere").
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/link_chips.rs (the chip drawing),
//! apps/desktop/src/app/window/session_links_panel.rs (the list).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::Tooltip;

use crate::app::helpers::*;
use crate::app::native_sidebar::link_chips::{
    open_session_link_command, session_link_chip_body, session_links_overflow_pill,
};
use crate::*;
use ghostex_gx_core::{SessionKey, SessionLinks};

/// Chips the header shows before its "+N".
const HEADER_CHIPS: usize = 3;
const HEADER_CHIPS_COMPACT: usize = 1;

impl GhostexGpuiApp {
    /// The session the header names: the focused Agents pane's active tab, on this computer.
    pub(crate) fn workarea_header_session_key(&self) -> Option<SessionKey> {
        let workspace = &self.agents_workspace;
        let pane_id = workspace
            .focus_mode_pane
            .into_iter()
            .chain(std::iter::once(workspace.focused_pane))
            .chain(workspace.rendered_leaf_order())
            .find(|pane_id| workspace.find_leaf(*pane_id).is_some())?;
        let session_id = workspace.active_session_in_pane(pane_id)?;
        let key = self.agents_chat_local_key_for_session(session_id)?;
        Some(SessionKey::local(key.project_id, key.session_id))
    }

    /// A session's links as gxserver last published them.
    pub(crate) fn session_links_of(&self, key: &SessionKey) -> Option<SessionLinks> {
        let session = self.gx_store.core.presentation().session(key)?;
        session
            .links
            .as_ref()
            .map(SessionLinks::from_presentation)
            .filter(|links| !links.is_empty())
    }

    /// Opens the session's links list under `anchor`.
    pub(crate) fn open_session_links_list(
        &mut self,
        key: SessionKey,
        anchor: Bounds<Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let reopen = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::SessionLinks)
            && self.native_sidebar.links_list_session.as_ref() == Some(&key);
        if reopen {
            self.set_gpui_titlebar_popup_open(
                GpuiTitlebarPopupKind::SessionLinks,
                false,
                None,
                window,
                cx,
            );
            return;
        }
        if self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::SessionLinks) {
            self.set_gpui_titlebar_popup_open(
                GpuiTitlebarPopupKind::SessionLinks,
                false,
                None,
                window,
                cx,
            );
        }
        self.native_sidebar.links_list_session = Some(key);
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::SessionLinks,
            true,
            Some(anchor),
            window,
            cx,
        );
    }

    pub(crate) fn render_workarea_header_session_links(
        &self,
        compact: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let key = self.workarea_header_session_key()?;
        let links = self.session_links_of(&key)?;
        let shown = links.items.len().min(if compact {
            HEADER_CHIPS_COMPACT
        } else {
            HEADER_CHIPS
        });
        let hidden = links.items.len() - shown;
        let border = chrome_ink().opacity(0.14);
        let chips = links
            .items
            .iter()
            .take(shown)
            .enumerate()
            .map(|(index, link)| {
                let open = open_session_link_command(link);
                let tooltip = format!("{}\n{}", link.summary(), link.detail_line());
                h_flex()
                    .id(format!("workarea-header-session-link-{index}"))
                    .flex_shrink_0()
                    .h(px(20.0))
                    .px(px(7.0))
                    .rounded_full()
                    .border_1()
                    .border_color(border)
                    .hover(|chip| chip.bg(chrome_ink().opacity(0.08)))
                    .child(session_link_chip_body(
                        link,
                        1.0,
                        11.5,
                        titlebar_text_color().into(),
                    ))
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                    .when_some(open, |chip, open| {
                        chip.cursor_pointer()
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.dispatch_native_sidebar_ui(open.clone(), cx);
                            }))
                    })
                    .into_any_element()
            });
        // "+N" for the links that did not fit, else a list button: the list is also where a link
        // is removed and where the chat's suggestions wait.
        let more_label = if hidden > 0 {
            None
        } else if !links.suggested.is_empty() {
            Some(format!("{} suggested", links.suggested.len()))
        } else {
            Some("List".to_string())
        };
        let failing = links.overflow_has_failing(shown);
        let more = {
            let key = key.clone();
            let pill = match &more_label {
                None => session_links_overflow_pill(
                    hidden,
                    failing,
                    1.0,
                    11.5,
                    titlebar_text_color().into(),
                    border.into(),
                )
                .h(px(20.0))
                .px(px(8.0)),
                Some(label) => div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .h(px(20.0))
                    .px(px(8.0))
                    .rounded_full()
                    .border_1()
                    .border_color(border)
                    .text_size(px(11.5))
                    .text_color(titlebar_disabled_text_color())
                    .child(label.clone()),
            };
            pill.id("workarea-header-session-links-more")
                .cursor_pointer()
                .hover(|pill| pill.bg(chrome_ink().opacity(0.08)))
                .tooltip(|window, cx| Tooltip::new("All links of this session").build(window, cx))
                .on_click(
                    cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                        let anchor = Bounds::new(event.position(), gpui::size(px(1.0), px(1.0)));
                        app.open_session_links_list(key.clone(), anchor, window, cx);
                    }),
                )
                .into_any_element()
        };
        Some(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .ml(px(10.0))
                .children(chips)
                .child(more)
                .into_any_element(),
        )
    }
}
