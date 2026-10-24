//! A session's links list: the dropdown the header's and a hovered card's "+N" open. Every PR,
//! GitHub issue and Linear ticket the session is linked to, on two lines each, with the chat's
//! suggestions below.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "i also want the user to be able to click on an x icon button (twice to confirm) to hide the link". The first click on an entry's × turns it into a red "Remove?" for three seconds; a second click removes the link, and it never joins the session again by itself (server/src/session_links/store.rs).
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: a PR or issue URL the chat only mentioned is offered under Suggested ("suggest") with a Link button, rather than linked by itself.

use super::resources_style::*;
use crate::app::helpers::*;
use crate::app::native_sidebar::link_chips::{
    open_session_link_command, session_link_glyph_icon, session_link_tone_color,
};
use crate::*;
use ghostex_gx_core::{SessionKey, SessionLink, SessionLinkKind, SessionLinks};
use serde_json::json;

/// How long a × waits for its confirming click.
const REMOVE_ARMED_FOR: Duration = Duration::from_secs(3);
pub(crate) const SESSION_LINKS_PANEL_WIDTH: f32 = 440.0;
const HEADER_HEIGHT: f32 = 38.0;
const SECTION_HEIGHT: f32 = 24.0;
const ROW_HEIGHT: f32 = 46.0;
const SUGGESTED_ROW_HEIGHT: f32 = 34.0;

/// The dropdown's height for a list, before the window caps it.
pub(crate) fn session_links_panel_height(links: Option<&SessionLinks>) -> f32 {
    let Some(links) = links else {
        return HEADER_HEIGHT + 60.0;
    };
    let sections = [
        SessionLinkKind::PullRequest,
        SessionLinkKind::Issue,
        SessionLinkKind::Linear,
    ]
    .iter()
    .filter(|kind| links.items.iter().any(|item| item.kind == **kind))
    .count()
        + usize::from(!links.suggested.is_empty());
    HEADER_HEIGHT
        + 16.0
        + sections as f32 * SECTION_HEIGHT
        + links.items.len() as f32 * ROW_HEIGHT
        + links.suggested.len() as f32 * SUGGESTED_ROW_HEIGHT
}

pub(crate) struct SessionLinksPanel {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    key: SessionKey,
    /// The entry whose × was clicked once, waiting for the second click.
    armed: Option<String>,
    scroll: ScrollHandle,
    _observe: Option<gpui::Subscription>,
}

impl SessionLinksPanel {
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        key: SessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // The list follows the session while it is open: a link the background pass finds, a
        // check that turns red, a removal.
        let observe = main_app
            .upgrade()
            .map(|app| cx.observe(&app, |_, _, cx| cx.notify()));
        Self {
            main_app,
            key,
            armed: None,
            scroll: ScrollHandle::new(),
            _observe: observe,
        }
    }

    fn links(&self, cx: &gpui::App) -> Option<SessionLinks> {
        self.main_app
            .upgrade()
            .and_then(|app| app.read(cx).session_links_of(&self.key))
    }

    fn close(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.clear_gpui_titlebar_popup_from_window(GpuiTitlebarPopupKind::SessionLinks, cx)
        });
        window.remove_window();
    }

    fn send(&self, command: serde_json::Value, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.dispatch_native_sidebar_ui(command, cx);
        });
    }

    fn click_remove(&mut self, key: String, cx: &mut gpui::Context<Self>) {
        if self.armed.as_deref() == Some(&key) {
            self.armed = None;
            let sidebar_id = self.key.to_sidebar_session_id();
            self.send(
                json!({ "type": "removeSessionLink", "sessionId": sidebar_id, "key": key }),
                cx,
            );
        } else {
            self.armed = Some(key.clone());
            cx.spawn(async move |panel, cx| {
                cx.background_executor().timer(REMOVE_ARMED_FOR).await;
                let _ = panel.update(cx, |panel, cx| {
                    if panel.armed.as_deref() == Some(&key) {
                        panel.armed = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn render_row(&self, link: &SessionLink, cx: &mut gpui::Context<Self>) -> AnyElement {
        let (glyph, tone) = link.glyph();
        let tone_color = session_link_tone_color(tone);
        let armed = self.armed.as_deref() == Some(link.key.as_str());
        let open = open_session_link_command(link);
        let key = link.key.clone();
        let title = link.title.clone().unwrap_or_default();
        let detail = link.detail_line();
        let failing = link.checks.as_deref() == Some("failing");
        h_flex()
            .id(gpui::SharedString::from(format!(
                "session-link-{}",
                link.key
            )))
            .h(px(ROW_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .items_start()
            .gap(px(8.0))
            .px(px(8.0))
            .pt(px(6.0))
            .rounded(px(6.0))
            .when(link.finished, |row| row.opacity(0.62))
            .hover(|row| row.bg(chrome_ink().opacity(0.07)))
            .child(div().pt(px(2.0)).child(titlebar_svg_icon(
                session_link_glyph_icon(glyph),
                14.0,
                tone_color,
            )))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.0))
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap(px(6.0))
                            .text_size(px(12.5))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(chrome_ink().opacity(0.96))
                                    .child(link.label.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(chrome_ink().opacity(0.86))
                                    .child(title),
                            )
                            .when_some(link.state_label(), |line, state| {
                                line.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(11.5))
                                        .text_color(tone_color)
                                        .child(state),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(if failing {
                                session_link_tone_color(ghostex_gx_core::SessionLinkTone::Closed)
                            } else {
                                chrome_ink().opacity(0.55).into()
                            })
                            .child(detail),
                    ),
            )
            .child(
                h_flex()
                    .id(gpui::SharedString::from(format!(
                        "session-link-remove-{}",
                        link.key
                    )))
                    .flex_shrink_0()
                    .h(px(20.0))
                    .px(px(if armed { 7.0 } else { 4.0 }))
                    .items_center()
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .when(armed, |button| {
                        button
                            .bg(
                                session_link_tone_color(ghostex_gx_core::SessionLinkTone::Closed)
                                    .opacity(0.16),
                            )
                            .text_color(session_link_tone_color(
                                ghostex_gx_core::SessionLinkTone::Closed,
                            ))
                            .child("Remove?")
                    })
                    .when(!armed, |button| {
                        button
                            .text_color(chrome_ink().opacity(0.5))
                            .hover(|button| button.bg(chrome_ink().opacity(0.12)))
                            .child(titlebar_svg_icon(
                                "titlebar/x.svg",
                                12.0,
                                chrome_ink().opacity(0.6).into(),
                            ))
                    })
                    .tooltip(move |window, cx| {
                        Tooltip::new(if armed {
                            "Click again to remove the link"
                        } else {
                            "Remove from this session"
                        })
                        .build(window, cx)
                    })
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        cx.stop_propagation();
                        panel.click_remove(key.clone(), cx);
                    })),
            )
            .when_some(open, |row, open| {
                row.cursor_pointer()
                    .on_click(cx.listener(move |panel, _, window, cx| {
                        panel.send(open.clone(), cx);
                        panel.close(window, cx);
                    }))
            })
            .into_any_element()
    }

    fn section(title: &str, count: usize) -> AnyElement {
        h_flex()
            .h(px(SECTION_HEIGHT))
            .px(px(8.0))
            .gap(px(6.0))
            .items_end()
            .pb(px(4.0))
            .text_size(px(11.0))
            .text_color(chrome_ink().opacity(0.55))
            .child(title.to_string())
            .child(count.to_string())
            .into_any_element()
    }
}

impl Render for SessionLinksPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let links = self.links(cx).unwrap_or_default();
        let open = links.items.iter().filter(|item| !item.finished).count();
        let failing = links
            .items
            .iter()
            .filter(|item| item.checks.as_deref() == Some("failing"))
            .count();
        let summary = [
            Some(format!("{open} open")),
            (failing > 0).then(|| format!("{failing} failing checks")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let mut body = v_flex().w_full().gap(px(2.0));
        for (kind, title) in [
            (SessionLinkKind::PullRequest, "Pull requests"),
            (SessionLinkKind::Issue, "GitHub issues"),
            (SessionLinkKind::Linear, "Linear"),
        ] {
            let items: Vec<&SessionLink> = links
                .items
                .iter()
                .filter(|item| item.kind == kind)
                .collect();
            if items.is_empty() {
                continue;
            }
            body = body.child(Self::section(title, items.len()));
            for item in items {
                body = body.child(self.render_row(item, cx));
            }
        }
        if !links.suggested.is_empty() {
            body = body.child(Self::section("Suggested", links.suggested.len()));
            for suggestion in &links.suggested {
                let (glyph, tone) = suggestion.glyph();
                let link = suggestion.url.clone().unwrap_or_default();
                let sidebar_id = self.key.to_sidebar_session_id();
                body = body.child(
                    h_flex()
                        .h(px(SUGGESTED_ROW_HEIGHT - 4.0))
                        .mb(px(4.0))
                        .w_full()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .items_center()
                        .rounded(px(6.0))
                        .border_1()
                        .border_dashed()
                        .border_color(chrome_ink().opacity(0.16))
                        .text_size(px(12.0))
                        .child(titlebar_svg_icon(
                            session_link_glyph_icon(glyph),
                            14.0,
                            session_link_tone_color(tone),
                        ))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(chrome_ink().opacity(0.9))
                                .child(suggestion.label.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(chrome_ink().opacity(0.55))
                                .child(
                                    suggestion
                                        .title
                                        .clone()
                                        .unwrap_or_else(|| "Mentioned in the chat".to_string()),
                                ),
                        )
                        .child(
                            div()
                                .id(gpui::SharedString::from(format!(
                                    "session-link-add-{}",
                                    suggestion.key
                                )))
                                .flex_shrink_0()
                                .px(px(8.0))
                                .h(px(20.0))
                                .flex()
                                .items_center()
                                .rounded(px(5.0))
                                .border_1()
                                .border_color(chrome_ink().opacity(0.18))
                                .text_size(px(11.5))
                                .text_color(chrome_ink().opacity(0.92))
                                .cursor_pointer()
                                .hover(|button| button.bg(chrome_ink().opacity(0.12)))
                                .child("Link")
                                .on_click(cx.listener(move |panel, _, _, cx| {
                                    panel.send(
                                        json!({
                                            "type": "addSessionLink",
                                            "sessionId": sidebar_id,
                                            "link": link,
                                        }),
                                        cx,
                                    );
                                })),
                        ),
                );
            }
        }
        if links.items.is_empty() && links.suggested.is_empty() {
            body = body.child(
                div()
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .text_color(chrome_ink().opacity(0.55))
                    .child("This session has no links."),
            );
        }
        resource_panel_frame().child(
            v_flex()
                .size_full()
                .overflow_hidden()
                .child(
                    resource_header().h(px(HEADER_HEIGHT)).child(
                        resource_heading()
                            .text_size(px(13.0))
                            .child(div().child("Links"))
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_size(px(12.0))
                                    .text_color(chrome_ink().opacity(0.55))
                                    .child(summary),
                            ),
                    ),
                )
                .child(
                    div().relative().w_full().min_h_0().flex_1().child(
                        v_flex()
                            .id("session-links-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .p(px(6.0))
                            .child(body),
                    ),
                ),
        )
    }
}
