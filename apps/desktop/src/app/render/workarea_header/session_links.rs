//! The work area header's link chips: on a line under the session's title, its PRs, issues and
//! Linear tickets, then "+N" (or a list button) that opens the whole list.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User, of the chips in the work area header: "yes yes yes love this". Up to three chips (one when the header is compact), open work first. The chips show for every session, not only in Work mode (the user's "everywhere").
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "let's please show the option to link a thread with issues/prs etc under the name of the thread instead of on the same thread title row". The chips and their "+N" / "N suggested" / "List" pill sit on a slim line right under the header row, starting where the title starts, only while the session has links or suggestions; the header row keeps its height and the title no longer gives way to them. The column below starts under that line (overlap.rs), as the phone's chip strip sits under its chat header. This supersedes the same day's chips after the title on the header row.
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/link_chips.rs (the chip drawing),
//! apps/desktop/src/app/native_sidebar/links_list.rs (the list).

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
use ghostex_gx_core::SessionKey;

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
                let tooltip = [link.summary(), link.detail_line()]
                    .into_iter()
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
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
                .children(chips)
                .child(more)
                .into_any_element(),
        )
    }

    /// The links line's height: its own while the header's session has links or suggestions, else
    /// nothing, so a session without any draws no empty strip. An expanded view panel shrinks the
    /// band to its tab strip, which leaves no header row to sit under.
    pub(crate) fn workarea_header_links_strip_height(&self) -> f32 {
        if self.workarea_header_hosts_view_tab_strip() && self.view_panel_maximized() {
            return 0.0;
        }
        let has_links = self
            .workarea_header_session_key()
            .and_then(|key| self.session_links_of(&key))
            .is_some_and(|links| !links.items.is_empty() || !links.suggested.is_empty());
        if has_links {
            WORKAREA_HEADER_LINKS_STRIP_HEIGHT
        } else {
            0.0
        }
    }

    /// The line under the header row: the chips from where the title starts. It paints the
    /// header's colour and takes the mouse like the row, so a transcript passing under the header
    /// passes under it too.
    pub(crate) fn render_workarea_header_links_strip(
        &self,
        compact: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if self.workarea_header_links_strip_height() == 0.0 {
            return None;
        }
        let chips = self.render_workarea_header_session_links(compact, cx)?;
        Some(
            h_flex()
                .id("ghostex-gpui-workarea-header-links")
                .w_full()
                .h(px(WORKAREA_HEADER_LINKS_STRIP_HEIGHT))
                .flex_shrink_0()
                .items_start()
                .overflow_hidden()
                .bg(self.workarea_header_surface_color())
                .when(self.workarea_header_blocks_mouse(), |strip| strip.occlude())
                .font_family(crate::ui_fonts::UI_FONT)
                // The breadcrumb label's own 3px inset puts the project icon, and the title, here.
                .pl(px(super::anchor::workarea_header_title_offset_x() + 3.0))
                .pr(px(WORKAREA_HEADER_EDGE_PADDING))
                .child(chips)
                .into_any_element(),
        )
    }
}
