//! The pills on the working card (working_card.rs). Always leftmost, two icon pills: "Scroll to
//! bottom" and "Previous question". Then Threads, Tasks, Subagents and Background, whose labels,
//! counts and order come from the core (`chatPills`, packages/gx-chat-core/src/extras/chat_pills.rs,
//! where the user's decision lives): a summary, since opening the card shows each one's section with
//! the rows these menus' builders draw (coordinator_threads.rs, agent_tasks.rs, agent_fleet.rs,
//! background_tasks.rs). The menu helpers here also serve the Files and links menu
//! (`option_menu/pill_menu.rs`).
//!
//! CDXC:SessionChat 2026-10-17 SEE-ALSO:
//! The phone draws the same pills in apps/mobile/app/src/chat/native/cards/ChatPills.tsx.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontFeatures, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb, svg,
};
use serde_json::Value;

/// A section's header line, the gap between its parts and its footer line ("N more"), unscaled.
pub(super) const MENU_HEADER_HEIGHT: f32 = 24.0;
pub(super) const MENU_SECTION_GAP: f32 = 4.0;
pub(super) const MENU_FOOTER_HEIGHT: f32 = 22.0;

/// The amber of a thread waiting on the user's approval (coordinator_threads.rs).
pub(super) const ATTENTION_COLOR: u32 = 0xf5a524;

fn icon(id: &str) -> &'static str {
    match id {
        "threads" => "titlebar/users-group.svg",
        "tasks" => "titlebar/list-check.svg",
        "background" => "titlebar/terminal-2.svg",
        _ => "titlebar/users.svg",
    }
}

/// One panel pill in the Worked row's chip look (completed_work_row.rs): icon, label, count, and
/// the amber dot when something in it needs the user. It has no menu of its own: a press reaches the
/// card, which opens to every section.
pub(super) fn chat_pill(pill: &Value, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let id = text(pill, "id");
    let count = text(pill, "count");
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(5.0 * s))
        .px(px(8.0 * s))
        .py(px(2.0 * s))
        .rounded_full()
        .border(px(s))
        .border_color(p.foreground.opacity(0.1))
        .text_size(px(11.5 * s))
        .line_height(px(16.0 * s))
        .whitespace_nowrap()
        .text_color(p.card_muted)
        .child(
            svg()
                .path(icon(&id))
                .size(px(12.0 * s))
                .flex_shrink_0()
                .text_color(p.card_muted),
        )
        .child(text(pill, "label"))
        .when(!count.is_empty(), |pill| {
            pill.child(
                div()
                    .text_color(p.foreground)
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .child(count),
            )
        })
        .when(pill["attention"] == true, |pill| {
            pill.child(
                div()
                    .size(px(6.0 * s))
                    .rounded_full()
                    .flex_shrink_0()
                    .bg(rgb(ATTENTION_COLOR)),
            )
        })
        .into_any_element()
}

/// A menu's first line: the panel's icon and title, its muted detail, and an optional ornament.
///
/// CDXC:SessionChat 2026-10-11 DECISION: User: the muted detail ("3 working · 207 threads") is the same size as the title beside it, still muted, on every panel that shares this header. (Moved here from the deleted panel_card.rs when the panels became pill menus, 2026-10-15.)
pub(super) fn pill_menu_header(
    icon: &'static str,
    title: &'static str,
    meta: String,
    trailing: Option<AnyElement>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    div()
        .flex_shrink_0()
        .h(px(MENU_HEADER_HEIGHT * s))
        .flex()
        .items_center()
        .gap(px(8.0 * s))
        .px(px(4.0 * s))
        .text_size(px(13.0 * s))
        .child(
            svg()
                .path(icon)
                .size(px(14.0 * s))
                .text_color(p.muted)
                .flex_shrink_0(),
        )
        .child(div().flex_shrink_0().text_color(p.foreground).child(title))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(p.card_muted)
                .child(meta),
        )
        .children(trailing)
        .into_any_element()
}

/// A menu's column: header, list and footer, `MENU_SECTION_GAP` apart.
pub(super) fn pill_menu_column(
    id: &'static str,
    label: &'static str,
    header: AnyElement,
    list: Option<AnyElement>,
    footer: Option<AnyElement>,
    p: &ChatAppearance,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::Group)
        .aria_label(label)
        .w_full()
        .flex()
        .flex_col()
        .gap(px(MENU_SECTION_GAP * p.scale))
        .child(header)
        .children(list)
        .children(footer)
        .into_any_element()
}

/// An icon-only pill, the label pills' height: built the way they are (a 16px line, 2px above and
/// below, a 1px edge), so the two never drift apart under a zoom.
fn icon_pill(
    id: &'static str,
    icon: &'static str,
    color: gpui::Hsla,
    p: &ChatAppearance,
) -> gpui::Stateful<gpui::Div> {
    let s = p.scale;
    div()
        .id(id)
        .tab_index(0)
        .role(gpui::Role::Button)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(28.0 * s))
        .py(px(2.0 * s))
        .rounded_full()
        .border(px(s))
        .chat_cursor_pointer()
        .focus_visible(|style| style.border_color(p.ring))
        .child(
            div()
                .h(px(16.0 * s))
                .flex()
                .items_center()
                .child(svg().path(icon).size(px(14.0 * s)).text_color(color)),
        )
}

impl NativeChatView {
    /// "Previous question" as a ? pill in the Threads pill's neutral look with its amber attention
    /// dot, while the chat has unanswered questions; a click jumps to the previous one
    /// (open_questions.rs), and the dot pings now and then (question_ping.rs).
    ///
    /// CDXC:SessionChat 2026-10-17 DECISION:
    /// User (2026-10-15): "make the Question pill and scroll to bottom pill not floating, make them appear as the leftmost in the pills row and they should both just be icons. (down arrow for scroll to bottom and ? icon for the question button) always leftmost pls", then "make the scroll to bottom appear left most so it doesn't push stuff when it appears", and "I also want the previous question pill to have same height as the threads and scroll to bottom pills (and same color)". So both are icon pills at the left end of the working row's pills, scroll-to-bottom first, built like the label pills, with the ? pill in the Threads pill's edge, icon colour and hover. Then (2026-10-17): "make the Previous question button animate every once in a while to indicate to the user that there's something there", picking the attention dot ping (Q236 G) every 30 s (Q237): the pill keeps that neutral look and adds the Threads pill's amber attention dot, which pings (question_ping.rs, where the cadence lives). Supersedes the 2026-10-15 pill with no dot; the floating spot over the transcript's bottom edge (2026-09-23, 2026-10-14) stays gone, the Escape and "Agent was interrupted" toasts keep it (scroll_bottom.rs). The pills now ride on the working card (working_card.rs, "on the pill", Q259). The phone does the same in ChatPills.tsx.
    pub(super) fn previous_question_icon_pill(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        self.open_question_pill_text()?;
        let color = p.card_muted;
        let label = text(&self.snapshot["openQuestions"], "accessibilityLabel");
        Some(
            icon_pill(
                "chat-previous-question",
                "titlebar/question-mark.svg",
                color,
                p,
            )
            .relative()
            .aria_label(label)
            .border_color(p.foreground.opacity(0.1))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Previous question").build(window, cx)
            })
            .on_hover(
                cx.listener(|chat, hovered: &bool, _, cx| chat.question_pill_hovered(*hovered, cx)),
            )
            .on_click(cx.listener(|chat, _, _, cx| {
                cx.stop_propagation();
                chat.previous_question(cx)
            }))
            .children(super::question_ping::attention_dot(
                self.question_ping_frame(),
                p,
            ))
            .into_any_element(),
        )
    }

    /// "Scroll to bottom" as a down-arrow pill, while the reader is away from the bottom; its tooltip
    /// names it and its shortcut (scroll_bottom.rs).
    ///
    /// CDXC:SessionChat 2026-10-17 DECISION:
    /// User: "make the scroll to bottom pill always appear on the leftmost so it doesn't push other pills when it appears". On the working card, in the lane or over the transcript's bottom edge, it takes the leftmost place among the card's pills and timers, which are laid out from the card's right edge, so appearing moves none of them; the label gives up the room. With nothing on the card it stands alone at the card's right end.
    pub(super) fn scroll_bottom_icon_pill(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !super::scroll_bottom::transcript_left_bottom(&self.list) {
            return None;
        }
        let label = super::scroll_bottom::label().to_string();
        let tooltip = label.clone();
        let shortcut = super::scroll_bottom::shortcut_label().unwrap_or_default();
        Some(
            icon_pill(
                "chat-scroll-bottom",
                "titlebar/arrow-down.svg",
                p.primary,
                p,
            )
            .aria_label(label)
            .border_color(p.foreground.opacity(0.1))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone())
                    .keys(shortcut.clone())
                    .build(window, cx)
            })
            .on_click(cx.listener(|chat, _, _, cx| {
                cx.stop_propagation();
                chat.jump_to_bottom(cx)
            }))
            .into_any_element(),
        )
    }

    /// The Files and links menu's content height (unscaled), capped at 60% of the pane; its list
    /// scrolls inside.
    pub(super) fn pill_menu_height(&self, id: &str) -> Option<f32> {
        if id != "filesLinks" {
            return None;
        }
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let pane = f32::from(self.bounds.get().size.height) / scale;
        super::files_links::files_links_menu_height(
            &self.snapshot["filesLinks"],
            (pane * 0.6).max(160.0),
        )
    }

    /// The body the Files and links menu draws, built with this view's listeners so its rows act on
    /// this chat; `window` is the menu's own.
    pub(super) fn pill_menu_body(
        &mut self,
        id: &str,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match id {
            "filesLinks" => self.files_links_menu_body(p, window, cx),
            _ => None,
        }
    }

    /// Closes the open menu, after a row that leaves the chat (a thread opened).
    pub(super) fn close_pill_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.option_menu.clone() {
            menu.update(cx, |menu, cx| menu.close(None, cx));
        }
    }

    /// A menu opened against a trigger went away.
    pub(super) fn pill_menu_closed(&mut self, trigger: &'static str, cx: &mut Context<Self>) {
        if trigger == super::files_links::FILES_LINKS_TRIGGER {
            self.files_links_menu_closed(cx);
        }
    }
}
