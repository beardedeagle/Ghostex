//! A session's links list: every PR, GitHub issue and Linear ticket the session is linked to, on
//! two lines each, with the chat's suggestions below. The header's "+N" dropdown and the floating
//! card a hovered sidebar row opens both draw it (the titlebar popup plumbing hosts the dropdown,
//! window/session_links_card.rs the card; the web build draws the card in its own page).
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "i also want the user to be able to click on an x icon button (twice to confirm) to hide the link". The first click on an entry's × turns it into a red "Remove?" for three seconds; a second click removes the link, and it never joins the session again by itself (server/src/session_links/store.rs).
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: a PR or issue URL the chat only mentioned is offered under Suggested ("suggest") with a Link button, rather than linked by itself.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "when a link is gone i see that i can't bring it back why not?", "i mean when i hit x button twice on it". Right after the second click the row reads "Removed · Undo" for a few seconds (this app's toasts have no buttons, so the undo stays in the row), and a collapsed "Removed (N)" group at the bottom lists every removed link with Restore (`/api/restoreSessionLink`).
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "can u pls make all the sections in these cards colapsible with chevron? please add this for pr and github issues etc (the headers there". Every section header (Pull requests, GitHub issues, Linear, Suggested, Removed) has the same chevron and folds on a click, keeping its count. The folded sections are one app-wide choice per section kind (`sessionLinksFoldedSections`, Removed folded until opened), kept across sessions and restarts in the settings file the way the chat keeps its zoom; the phone keeps its own (SessionLinksSheet.tsx).
//! CDXC:SessionLinks 2026-10-24 WHY:
//! The list opens from a click or a hover the app is still handling, and opening a window draws it at once, so a list that read the app while drawing crashed Ghostex ("cannot read GhostexGpuiApp while it is already being updated"). The opener hands the list in, and later changes arrive through the observer, which runs after the app's update.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, ScrollHandle, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, v_flex};
use serde_json::json;
use std::time::Duration;

use super::link_chips::{
    open_session_link_command, session_link_glyph_icon, session_link_tone_color,
};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use ghostex_gx_core::{SessionKey, SessionLink, SessionLinkKind, SessionLinkTone, SessionLinks};

/// How long a × waits for its confirming click.
const REMOVE_ARMED_FOR: Duration = Duration::from_secs(3);
/// How long a removed row offers Undo.
const UNDO_FOR: Duration = Duration::from_secs(6);
pub(crate) const SESSION_LINKS_LIST_WIDTH: f32 = 440.0;
/// The floating card beside the sidebar is narrower than the header's dropdown.
pub(crate) const SESSION_LINKS_CARD_WIDTH: f32 = 380.0;
const HEADER_HEIGHT: f32 = 38.0;
const SECTION_HEIGHT: f32 = 24.0;
const ROW_HEIGHT: f32 = 46.0;
const SUGGESTED_ROW_HEIGHT: f32 = 34.0;

/// Where a list is shown, which decides what closing it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionLinksListHost {
    /// The header's "+N" dropdown.
    Dropdown,
    /// The card a hovered sidebar row opens.
    Card,
}

/// The list's height for these links, before the window caps it.
pub(crate) fn session_links_list_height(links: Option<&SessionLinks>) -> f32 {
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
        + usize::from(!links.suggested.is_empty())
        + usize::from(!links.removed.is_empty());
    HEADER_HEIGHT
        + 16.0
        + sections as f32 * SECTION_HEIGHT
        + links.items.len() as f32 * ROW_HEIGHT
        + links.suggested.len() as f32 * SUGGESTED_ROW_HEIGHT
}

pub(crate) struct SessionLinksList {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    key: SessionKey,
    host: SessionLinksListHost,
    /// The entry whose × was clicked once, waiting for the second click.
    armed: Option<String>,
    /// The entry just removed, which reads "Removed · Undo" for a few seconds.
    undo: Option<SessionLink>,
    /// The sections folded shut, by id (`pullRequests`, `issues`, `linear`, `suggested`,
    /// `removed`), as `sessionLinksFoldedSections` says.
    folded: Vec<String>,
    scroll: ScrollHandle,
    /// The list this draws, kept here rather than read from the app while drawing.
    links: Option<SessionLinks>,
    _observe: Option<gpui::Subscription>,
}

impl SessionLinksList {
    /// `links` is the list as the app has it when the list opens.
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        key: SessionKey,
        host: SessionLinksListHost,
        links: Option<SessionLinks>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The list follows the session while it is open: a link the background pass finds, a
        // check that turns red, a removal.
        let observe = main_app.upgrade().map(|app| {
            cx.observe(&app, |list: &mut Self, app, cx| {
                let links = app.read(cx).session_links_of(&list.key);
                let folded = folded_session_link_sections();
                if links != list.links || folded != list.folded {
                    list.links = links;
                    list.folded = folded;
                    cx.notify();
                }
            })
        });
        Self {
            main_app,
            key,
            host,
            armed: None,
            undo: None,
            folded: folded_session_link_sections(),
            scroll: ScrollHandle::new(),
            links,
            _observe: observe,
        }
    }

    fn close(&self, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.host;
        let _ = self.main_app.update(cx, |app, cx| {
            app.close_session_links_host(host, window, cx);
        });
    }

    fn send(&self, command: serde_json::Value, cx: &mut Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.dispatch_native_sidebar_ui(command, cx);
        });
    }

    fn click_remove(&mut self, key: String, cx: &mut Context<Self>) {
        if self.armed.as_deref() == Some(&key) {
            self.armed = None;
            let sidebar_id = self.key.to_sidebar_session_id();
            self.undo = self
                .links
                .as_ref()
                .and_then(|links| links.items.iter().find(|link| link.key == key).cloned());
            self.send(
                json!({ "type": "removeSessionLink", "sessionId": sidebar_id, "key": key }),
                cx,
            );
            cx.spawn(async move |list, cx| {
                cx.background_executor().timer(UNDO_FOR).await;
                let _ = list.update(cx, |list, cx| {
                    if list.undo.as_ref().is_some_and(|undo| undo.key == key) {
                        list.undo = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        } else {
            self.armed = Some(key.clone());
            cx.spawn(async move |list, cx| {
                cx.background_executor().timer(REMOVE_ARMED_FOR).await;
                let _ = list.update(cx, |list, cx| {
                    if list.armed.as_deref() == Some(&key) {
                        list.armed = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn render_row(&self, link: &SessionLink, cx: &mut Context<Self>) -> AnyElement {
        let (glyph, tone) = link.glyph();
        let tone_color = session_link_tone_color(tone);
        let red = session_link_tone_color(SessionLinkTone::Closed);
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
                                red
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
                            .bg(red.opacity(0.16))
                            .text_color(red)
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
                    .on_click(cx.listener(move |list, _, _, cx| {
                        cx.stop_propagation();
                        list.click_remove(key.clone(), cx);
                    })),
            )
            .when_some(open, |row, open| {
                row.cursor_pointer()
                    .on_click(cx.listener(move |list, _, window, cx| {
                        list.send(open.clone(), cx);
                        list.close(window, cx);
                    }))
            })
            .into_any_element()
    }

    fn restore(&mut self, key: String, cx: &mut Context<Self>) {
        if self.undo.as_ref().is_some_and(|undo| undo.key == key) {
            self.undo = None;
        }
        let sidebar_id = self.key.to_sidebar_session_id();
        self.send(
            json!({ "type": "restoreSessionLink", "sessionId": sidebar_id, "key": key }),
            cx,
        );
        cx.notify();
    }

    /// A small text button: Undo, Restore.
    fn text_button(
        id: String,
        label: &'static str,
        on_click: impl Fn(&mut Self, &gpui::ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(gpui::SharedString::from(id))
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
            .child(label)
            .on_click(cx.listener(move |list, event, window, cx| {
                cx.stop_propagation();
                on_click(list, event, window, cx);
            }))
            .into_any_element()
    }

    /// The row of a link just removed: "Removed · #318 …" and Undo, for a few seconds.
    fn render_undo_row(&self, link: &SessionLink, cx: &mut Context<Self>) -> AnyElement {
        let key = link.key.clone();
        h_flex()
            .h(px(ROW_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .bg(chrome_ink().opacity(0.05))
            .text_size(px(12.0))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(chrome_ink().opacity(0.55))
                    .child("Removed ·"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(chrome_ink().opacity(0.7))
                    .child(match &link.title {
                        Some(title) => format!("{} {title}", link.label),
                        None => link.label.clone(),
                    }),
            )
            .child(Self::text_button(
                format!("session-link-undo-{}", link.key),
                "Undo",
                move |list, _, _, cx| list.restore(key.clone(), cx),
                cx,
            ))
            .into_any_element()
    }

    /// The collapsed "Removed (N)" group: its header toggles it, each row offers Restore.
    fn render_removed(&self, links: &SessionLinks, cx: &mut Context<Self>) -> Option<AnyElement> {
        let undo_key = self.undo.as_ref().map(|undo| undo.key.as_str());
        let removed: Vec<&ghostex_gx_core::SuggestedSessionLink> = links
            .removed
            .iter()
            .filter(|link| Some(link.key.as_str()) != undo_key)
            .collect();
        if removed.is_empty() {
            return None;
        }
        let open = !self.is_folded("removed");
        let mut group = v_flex().w_full().gap(px(2.0)).child(self.section_header(
            "removed",
            "Removed",
            removed.len(),
            cx,
        ));
        if open {
            for link in removed {
                let (glyph, _) = link.glyph();
                let key = link.key.clone();
                group = group.child(
                    h_flex()
                        .h(px(SUGGESTED_ROW_HEIGHT - 4.0))
                        .w_full()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .items_center()
                        .text_size(px(12.0))
                        .child(titlebar_svg_icon(
                            session_link_glyph_icon(glyph),
                            14.0,
                            chrome_ink().opacity(0.45).into(),
                        ))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(chrome_ink().opacity(0.75))
                                .child(link.label.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(chrome_ink().opacity(0.5))
                                .child(link.title.clone().unwrap_or_default()),
                        )
                        .child(Self::text_button(
                            format!("session-link-restore-{}", link.key),
                            "Restore",
                            move |list, _, _, cx| list.restore(key.clone(), cx),
                            cx,
                        )),
                );
            }
        }
        Some(group.into_any_element())
    }

    fn is_folded(&self, section: &str) -> bool {
        self.folded.iter().any(|folded| folded == section)
    }

    fn toggle_section(&mut self, section: &'static str, cx: &mut Context<Self>) {
        let fold = !self.is_folded(section);
        if fold {
            self.folded.push(section.to_string());
        } else {
            self.folded.retain(|folded| folded != section);
            // The last sections sit at the bottom of a list the window does not grow for, so
            // opening one scrolls its rows into view.
            if matches!(section, "suggested" | "removed") {
                self.scroll.scroll_to_bottom();
            }
        }
        self.send(
            json!({ "type": "setSessionLinksFolded", "section": section, "folded": fold }),
            cx,
        );
        cx.notify();
    }

    /// A section's header: the chevron, its title and its count, folding the section on a click.
    fn section_header(
        &self,
        section: &'static str,
        title: &str,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let folded = self.is_folded(section);
        h_flex()
            .id(gpui::SharedString::from(format!(
                "session-links-section-{section}"
            )))
            .h(px(SECTION_HEIGHT))
            .px(px(8.0))
            .items_end()
            .pb(px(4.0))
            .text_size(px(11.0))
            .text_color(chrome_ink().opacity(0.55))
            .cursor_pointer()
            .hover(|header| header.text_color(chrome_ink().opacity(0.85)))
            .role(gpui::Role::Button)
            .aria_label(format!("{title} {count}"))
            // The chevron centres on the text's own line: aligned to the bottom of the header it
            // sat on the line box's bottom, below the letters.
            .child(
                h_flex()
                    .gap(px(5.0))
                    .items_center()
                    .line_height(px(14.0))
                    .child(titlebar_svg_icon(
                        if folded {
                            "titlebar/chevron-right.svg"
                        } else {
                            "titlebar/chevron-down.svg"
                        },
                        11.0,
                        chrome_ink().opacity(0.55).into(),
                    ))
                    .child(title.to_string())
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |list, _, _, cx| list.toggle_section(section, cx)))
            .into_any_element()
    }

    fn render_suggestion(
        &self,
        suggestion: &ghostex_gx_core::SuggestedSessionLink,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (glyph, tone) = suggestion.glyph();
        let link = suggestion.url.clone().unwrap_or_default();
        let sidebar_id = self.key.to_sidebar_session_id();
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
                    .child(match &suggestion.title {
                        Some(title) => format!("{title} · Mentioned in the chat"),
                        None => "Mentioned in the chat".to_string(),
                    }),
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
                    .on_click(cx.listener(move |list, _, _, cx| {
                        list.send(
                            json!({
                                "type": "addSessionLink",
                                "sessionId": sidebar_id,
                                "link": link,
                            }),
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }
}

impl Render for SessionLinksList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let links = self.links.clone().unwrap_or_default();
        // Only links read as open count: one not read yet is neither open nor finished.
        let open = links
            .items
            .iter()
            .filter(|item| !item.finished && item.state.is_some())
            .count();
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
        for (kind, section, title) in [
            (
                SessionLinkKind::PullRequest,
                "pullRequests",
                "Pull requests",
            ),
            (SessionLinkKind::Issue, "issues", "GitHub issues"),
            (SessionLinkKind::Linear, "linear", "Linear"),
        ] {
            let undo = self.undo.clone().filter(|undo| undo.kind == kind);
            let items: Vec<&SessionLink> = links
                .items
                .iter()
                .filter(|item| item.kind == kind)
                .filter(|item| undo.as_ref().is_none_or(|undo| undo.key != item.key))
                .collect();
            if items.is_empty() && undo.is_none() {
                continue;
            }
            body = body.child(self.section_header(section, title, items.len(), cx));
            // A just-removed row keeps its Undo even while its section is folded.
            if let Some(undo) = &undo {
                body = body.child(self.render_undo_row(undo, cx));
            }
            if !self.is_folded(section) {
                for item in items {
                    body = body.child(self.render_row(item, cx));
                }
            }
        }
        if !links.suggested.is_empty() {
            body = body.child(self.section_header(
                "suggested",
                "Suggested",
                links.suggested.len(),
                cx,
            ));
            if !self.is_folded("suggested") {
                for suggestion in &links.suggested {
                    body = body.child(self.render_suggestion(suggestion, cx));
                }
            }
        }
        if let Some(removed) = self.render_removed(&links, cx) {
            body = body.child(removed);
        }
        if links.items.is_empty() && links.suggested.is_empty() && self.undo.is_none() {
            body = body.child(
                div()
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .text_color(chrome_ink().opacity(0.55))
                    .child("This session has no links."),
            );
        }
        let card = self.host == SessionLinksListHost::Card;
        div()
            .id("session-links-list")
            .role(gpui::Role::Dialog)
            .aria_label("Links")
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(px(8.0))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(popup_window_surface(titlebar_background()))
            .text_color(chrome_ink())
            // The card stays while the pointer is on it and closes once it leaves both the card
            // and its row (window/session_links_card.rs).
            .when(card, |list| {
                list.on_hover(cx.listener(|list, hovered: &bool, _, cx| {
                    let hovered = *hovered;
                    let _ = list.main_app.update(cx, |app, cx| {
                        app.set_session_links_card_hovered(hovered, cx);
                    });
                }))
            })
            .child(
                v_flex()
                    .size_full()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .h(px(HEADER_HEIGHT))
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(8.0))
                            .pl(px(12.0))
                            .border_b_1()
                            .border_color(chrome_ink().opacity(0.12))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(chrome_ink().opacity(0.96))
                            .child("Links")
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_size(px(12.0))
                                    .text_color(chrome_ink().opacity(0.55))
                                    .child(summary),
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

/// The sections folded shut, from `sessionLinksFoldedSections`; Removed starts folded.
fn folded_session_link_sections() -> Vec<String> {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    match settings
        .object()
        .get("sessionLinksFoldedSections")
        .and_then(serde_json::Value::as_array)
    {
        Some(sections) => sections
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect(),
        None => vec!["removed".to_string()],
    }
}
