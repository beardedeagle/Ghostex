//! A session's PR, issue and Linear chips: the line and the badge a sidebar row shows (as the
//! "Session links in the sidebar" setting says), and the chip drawing the work area header shares
//! (render/workarea_header/session_links.rs). Hovering a row opens the floating links card
//! (window/session_links_card.rs on the desktop).
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "also didn't we agree to have a way to see these floating on the right of the sidebar when i hover somewhere? (instead of showing them just below the sidebar item". Hovering a row that has links opens a card beside the sidebar listing them all, after the tooltip delay; the row no longer grows by a line on hover. This supersedes the earlier 2026-10-24 rule that a hovered card expands to show its links below it.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/links.rs (`SessionLinksDisplay` and what a row
//! shows, glyphs, tones and words), apps/desktop/src/app/gx_store/session_links.rs (what a click
//! sends), apps/mobile/app/src/chat/native/sessionLinks.ts (the phone's copy of the row rule).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::json;

use super::appearance::SidebarAppearance;
use super::model::NativeSidebarSession;
use super::tooltips::SidebarTooltipSpan;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use ghostex_gx_core::{
    SessionLink, SessionLinkGlyph, SessionLinkTone, SessionLinks, SessionLinksDisplay,
    SessionLinksRow,
};

/// The line's height, the Work mode card's chip line's (work_chips.rs).
pub(crate) const LINK_LINE_HEIGHT: f32 = 16.0;
/// Roughly how wide a chip draws at 11.5px, unscaled: glyph, gap, label, checks mark.
fn chip_width(link: &SessionLink) -> f32 {
    12.0 + 3.0
        + link.label.chars().count() as f32 * 6.6
        + if link.checks.is_some() { 15.0 } else { 0.0 }
}

/// How many chips fit in `available` unscaled pixels, keeping room for the "+N" pill when some
/// are left out.
pub(crate) fn session_link_chips_that_fit(
    items: &[SessionLink],
    available: f32,
    gap: f32,
) -> usize {
    const PILL: f32 = 34.0;
    let mut used = 0.0;
    for (index, link) in items.iter().enumerate() {
        let width = chip_width(link) + if index > 0 { gap } else { 0.0 };
        let pill = if index + 1 < items.len() {
            gap + PILL
        } else {
            0.0
        };
        if used + width + pill > available {
            return index;
        }
        used += width;
    }
    items.len()
}

pub(crate) fn session_link_glyph_icon(glyph: SessionLinkGlyph) -> &'static str {
    match glyph {
        SessionLinkGlyph::PullRequest => "titlebar/git-pull-request.svg",
        SessionLinkGlyph::PullRequestDraft => "titlebar/git-pull-request-draft.svg",
        SessionLinkGlyph::PullRequestClosed => "titlebar/git-pull-request-closed.svg",
        SessionLinkGlyph::Merged => "titlebar/git-merge.svg",
        SessionLinkGlyph::IssueOpen => "titlebar/circle-dot.svg",
        SessionLinkGlyph::IssueClosed | SessionLinkGlyph::ChecksPassing => {
            "titlebar/circle-check.svg"
        }
        SessionLinkGlyph::LinearBacklog => "titlebar/circle-dashed.svg",
        SessionLinkGlyph::LinearTodo => "titlebar/circle.svg",
        SessionLinkGlyph::LinearStarted => "titlebar/work-issue-started.svg",
        SessionLinkGlyph::LinearReview => "titlebar/work-issue-review.svg",
        SessionLinkGlyph::LinearDone => "titlebar/circle-check-filled.svg",
        SessionLinkGlyph::LinearCanceled | SessionLinkGlyph::ChecksFailing => {
            "titlebar/circle-x.svg"
        }
        SessionLinkGlyph::ChecksPending => "titlebar/loader2.svg",
    }
}

/// The work chips' colours (work_chips.rs), the same in both themes.
pub(crate) fn session_link_tone_color(tone: SessionLinkTone) -> Hsla {
    rgb(match tone {
        SessionLinkTone::Open => 0x3fb950,
        SessionLinkTone::Draft => 0x8b949e,
        SessionLinkTone::Merged => 0xa371f7,
        SessionLinkTone::Closed => 0xf85149,
        SessionLinkTone::Pending => 0xd29922,
        SessionLinkTone::Started => 0xf2c94c,
        SessionLinkTone::Review => 0x4cb782,
        SessionLinkTone::Done => 0x5e6ad2,
        SessionLinkTone::Linear => 0x7c84f0,
    })
    .into()
}

/// What clicking a chip sends: the link opens where Settings says (gx_store/session_links.rs).
pub(crate) fn open_session_link_command(link: &SessionLink) -> Option<serde_json::Value> {
    link.url
        .as_ref()
        .map(|url| json!({ "type": "openSessionLink", "url": url }))
}

/// One chip: the state glyph, the number or ID, and a PR's checks mark.
pub(crate) fn session_link_chip_body(
    link: &SessionLink,
    scale: f32,
    text_size: f32,
    muted: Hsla,
) -> gpui::Div {
    let (glyph, tone) = link.glyph();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.0 * scale))
        .text_size(px(text_size * scale))
        .text_color(muted)
        .whitespace_nowrap()
        .child(titlebar_svg_icon(
            session_link_glyph_icon(glyph),
            12.0 * scale,
            session_link_tone_color(tone),
        ))
        .child(link.label.clone())
        .when_some(link.checks_mark(), |chip, (glyph, tone)| {
            chip.child(titlebar_svg_icon(
                session_link_glyph_icon(glyph),
                12.0 * scale,
                session_link_tone_color(tone),
            ))
        })
}

/// The "+N" pill: the links that did not fit, with a red dot when one of them has failing checks.
///
/// CDXC:SessionLinks 2026-10-24 SEE-ALSO: the user's "dot-only" answer (server/src/server/session_links_sync.rs); this dot is how a failing PR behind "+N" still shows.
pub(crate) fn session_links_overflow_pill(
    hidden: usize,
    failing: bool,
    scale: f32,
    text_size: f32,
    foreground: Hsla,
    border: Hsla,
) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0 * scale))
        .h(px(15.0 * scale))
        .px(px(6.0 * scale))
        .rounded_full()
        .border_1()
        .border_color(border)
        .text_size(px(text_size * scale))
        .text_color(foreground)
        .whitespace_nowrap()
        .child(format!("+{hidden}"))
        .when(failing, |pill| {
            pill.child(
                div()
                    .size(px(5.0 * scale))
                    .rounded_full()
                    .bg(session_link_tone_color(SessionLinkTone::Closed)),
            )
        })
}

impl GhostexGpuiApp {
    /// A session's links as gxserver last published them.
    pub(crate) fn session_links_of(
        &self,
        key: &ghostex_gx_core::SessionKey,
    ) -> Option<SessionLinks> {
        let session = self.gx_store.core.presentation().session(key)?;
        session
            .links
            .as_ref()
            .map(SessionLinks::from_presentation)
            .filter(|links| !links.is_empty())
    }

    /// The "Session links in the sidebar" setting.
    pub(crate) fn native_session_links_display(&self) -> SessionLinksDisplay {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        SessionLinksDisplay::from_setting(
            settings
                .object()
                .get(SessionLinksDisplay::SETTING_KEY)
                .and_then(serde_json::Value::as_str),
        )
    }

    /// What this row shows of its links (gx-core's rule), the session open in the window counting
    /// as selected.
    pub(crate) fn native_session_links_row(
        &self,
        session: &NativeSidebarSession,
    ) -> SessionLinksRow {
        let Some(links) = session.links.as_deref() else {
            return SessionLinksRow::default();
        };
        let (selected, _) = self.gx_store_sidebar_row_focus(
            &session.session_id,
            session.is_browser(),
            session.is_focused,
            session.is_visible,
        );
        links.row(self.native_session_links_display(), selected)
    }

    /// Whether the row draws a second line: its links line, or a Work mode card's Clean up / Keep
    /// offer for a merged PR (work_chips.rs), which takes the line while it waits.
    pub(crate) fn native_session_has_second_line(&self, session: &NativeSidebarSession) -> bool {
        session.work.as_ref().is_some_and(|work| work.offer_cleanup)
            || !self.native_session_links_row(session).line.is_empty()
    }

    /// The badge a row shows with the Badge option: the number of links, coloured by the most
    /// pressing one; or, under every option, a session's suggestions while it has no links, muted
    /// and dashed. Hidden while the row is hovered, like the row's other badges.
    pub(super) fn render_native_session_links_badge(
        &self,
        session: &NativeSidebarSession,
        appearance: &SidebarAppearance,
    ) -> Option<AnyElement> {
        let badge = self.native_session_links_row(session).badge?;
        let scale = appearance.scale;
        let color = if badge.suggested {
            appearance.muted
        } else {
            session_link_tone_color(badge.tone)
        };
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(3.0 * scale))
                .h(px(16.0 * scale))
                .px(px(5.0 * scale))
                .rounded_full()
                .border_1()
                .when(badge.suggested, |badge| badge.border_dashed())
                .border_color(color.opacity(0.45))
                .text_size(px(10.5 * scale))
                .text_color(color)
                .child(titlebar_svg_icon("titlebar/link.svg", 10.0 * scale, color))
                .child(badge.count.to_string())
                .into_any_element(),
        )
    }

    /// The row's links line (the Always, Selected session and Open work only options): the chips
    /// gx-core picked that fit, then "+N" for the rest, which opens the floating card.
    pub(super) fn render_native_session_link_line(
        &self,
        session: &NativeSidebarSession,
        indent: f32,
        tooltip_span: SidebarTooltipSpan,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if session.work.as_ref().is_some_and(|work| work.offer_cleanup) {
            return None;
        }
        let row = self.native_session_links_row(session);
        if row.line.is_empty() {
            return None;
        }
        let links: &SessionLinks = session.links.as_deref()?;
        let items: Vec<SessionLink> = row
            .line
            .iter()
            .filter_map(|index| links.items.get(*index).cloned())
            .collect();
        let scale = appearance.scale;
        // The card's width less its insets and the indent under the title.
        let available = self.sidebar_width / scale - 2.0 * 3.0 - (indent + 15.0 + 6.0) - 6.0 - 8.0;
        let shown = session_link_chips_that_fit(&items, available, 10.0);
        let hidden = links.items.len() - shown;
        let show_tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let session_id = session.session_id.clone();
        let chips = items.iter().take(shown).enumerate().map(|(index, link)| {
            let click = open_session_link_command(link);
            let tooltip = format!("{}\n{}", link.summary(), link.detail_line());
            let foreground = appearance.foreground;
            session_link_chip_body(link, scale, 11.5, appearance.muted)
                .id(format!("native-session-link-{index}-{session_id}"))
                .role(gpui::Role::Link)
                .aria_label(link.summary())
                .when_some(click, |chip, click| {
                    chip.cursor_pointer()
                        .hover(move |style| {
                            style
                                .text_color(foreground)
                                .text_decoration_1()
                                .text_decoration_color(foreground)
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(click.clone(), cx);
                        }))
                })
                .when(show_tooltips, |chip| {
                    chip.managed_discrete_tooltip_with_placement(
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
                .into_any_element()
        });
        let overflow = (hidden > 0).then(|| {
            let key = ghostex_gx_core::SessionKey::parse_sidebar_session_id(&session_id)
                .filter(|key| key.machine.is_local());
            let failing = links.items.iter().any(|link| {
                link.checks.as_deref() == Some("failing")
                    && !items.iter().take(shown).any(|shown| shown.key == link.key)
            });
            session_links_overflow_pill(
                hidden,
                failing,
                scale,
                11.0,
                appearance.foreground,
                appearance.muted.opacity(0.45),
            )
            .id(format!("native-session-links-more-{session_id}"))
            .role(gpui::Role::Button)
            .aria_label(format!("{hidden} more links"))
            .when(key.is_some(), |pill| {
                let session_id = session_id.clone();
                pill.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.open_session_links_card(session_id.clone(), cx);
                    }))
            })
            .into_any_element()
        });
        Some(
            h_flex()
                .h(px(LINK_LINE_HEIGHT * scale))
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                // Under the title.
                .pl(px((indent + 15.0 + 6.0) * scale))
                .pr(px(6.0 * scale))
                .gap(px(10.0 * scale))
                .children(chips)
                .children(overflow)
                .into_any_element(),
        )
    }
}
