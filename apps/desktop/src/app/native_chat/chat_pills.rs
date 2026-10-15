//! The pills on the working row and the floating menu each one opens. Always leftmost, two icon
//! pills: "Scroll to bottom" and "Previous question". Then Threads, Tasks, Subagents and Background, whose
//! labels, counts and order come from the core (`chatPills`,
//! packages/gx-chat-core/src/extras/chat_pills.rs, where the user's decision lives); this lays the
//! row out and opens the menu, a chat option menu window (`option_menu/pill_menu.rs`) that draws the
//! panel's rows with this view's own row builders (coordinator_threads.rs, agent_tasks.rs,
//! agent_fleet.rs).
//!
//! CDXC:SessionChat 2026-10-15 SEE-ALSO:
//! The phone draws the same row and menus in apps/mobile/app/src/chat/native/cards/ChatPills.tsx.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontFeatures, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, Window, canvas, div,
    px, rgb, svg,
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

/// A menu's header line, the gap between its sections and its footer line ("N more"), unscaled.
pub(super) const MENU_HEADER_HEIGHT: f32 = 24.0;
pub(super) const MENU_SECTION_GAP: f32 = 4.0;
pub(super) const MENU_FOOTER_HEIGHT: f32 = 22.0;

/// Below this pane width (unscaled) the pills leave the working row for a line of their own under it.
/// Three pills and a working word need about 430px; the composer column is at most 768px wide.
pub(super) const PILLS_OWN_LINE_BELOW: f32 = 560.0;

/// The narrowest the pills' scroller gets beside the word before it moves to its own line.
const PILLS_MIN_BESIDE_WORD: f32 = 96.0;

/// The amber of a thread waiting on the user's approval (coordinator_threads.rs).
const ATTENTION_COLOR: u32 = 0xf5a524;

/// Each pill's frame at its last paint, which its menu opens against.
#[derive(Default)]
pub(super) struct ChatPillBounds {
    threads: Rc<Cell<Bounds<Pixels>>>,
    tasks: Rc<Cell<Bounds<Pixels>>>,
    subagents: Rc<Cell<Bounds<Pixels>>>,
    background: Rc<Cell<Bounds<Pixels>>>,
}

impl ChatPillBounds {
    fn cell(&self, id: &str) -> Rc<Cell<Bounds<Pixels>>> {
        match id {
            "threads" => self.threads.clone(),
            "tasks" => self.tasks.clone(),
            "background" => self.background.clone(),
            _ => self.subagents.clone(),
        }
    }
}

/// The menu-toggle key of a pill (menu_toggle.rs).
fn trigger_id(id: &str) -> &'static str {
    match id {
        "threads" => "chat-pill-threads",
        "tasks" => "chat-pill-tasks",
        "background" => "chat-pill-background",
        _ => "chat-pill-subagents",
    }
}

fn icon(id: &str) -> &'static str {
    match id {
        "threads" => "titlebar/users-group.svg",
        "tasks" => "titlebar/list-check.svg",
        "background" => "titlebar/terminal-2.svg",
        _ => "titlebar/users.svg",
    }
}

fn menu_width(id: &str) -> f32 {
    match id {
        "tasks" => 360.0,
        _ => 440.0,
    }
}

/// The menu's content height (unscaled) for `id`, or `None` when its panel is gone.
fn panel_menu_height(id: &str, snapshot: &Value) -> Option<f32> {
    let (key, height): (&str, fn(&Value) -> f32) = match id {
        "threads" => (
            "coordinatorThreadsPanel",
            super::coordinator_threads::threads_menu_height,
        ),
        "tasks" => ("agentTasksPanel", super::agent_tasks::tasks_menu_height),
        "subagents" => ("agentFleetStrip", super::agent_fleet::fleet_menu_height),
        "background" => (
            "backgroundTasksPanel",
            super::background_tasks::background_menu_height,
        ),
        _ => return None,
    };
    let panel = &snapshot[key];
    panel.is_object().then(|| height(panel))
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

/// An icon-only pill, the label pills' height (their 16px line plus 2px above and below).
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
        .h(px(22.0 * s))
        .w(px(28.0 * s))
        .rounded_full()
        .border(px(s))
        .chat_cursor_pointer()
        .focus_visible(|style| style.border_color(p.ring))
        .child(svg().path(icon).size(px(14.0 * s)).text_color(color))
}

impl NativeChatView {
    /// The pills' scroller: right-aligned, one line, scrolling sideways when the pills do not fit.
    /// `own_line` is the narrow layout, where it fills the line under the working word.
    pub(super) fn render_chat_pills(
        &self,
        own_line: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let pills = self.snapshot["chatPills"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let s = p.scale;
        // Always leftmost: scroll-to-bottom, then the question pill. The row is right-aligned, so a
        // pill appearing at its left end moves nothing else.
        let leading: Vec<AnyElement> = self
            .scroll_bottom_icon_pill(p, cx)
            .into_iter()
            .chain(self.previous_question_icon_pill(p, cx))
            .collect();
        if pills.is_empty() && leading.is_empty() {
            return None;
        }
        // The auto margin pushes the pills right while they fit and resolves to nothing once they
        // overflow, so the scroller then starts at the first pill instead of cutting it off.
        let row = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(6.0 * s))
            .ml_auto()
            .children(leading)
            .children(pills.iter().map(|pill| self.chat_pill(pill, p, cx)));
        Some(
            div()
                .id("chat-pills")
                .role(gpui::Role::Group)
                .aria_label("Chat panels")
                .flex()
                .items_center()
                .min_w_0()
                .overflow_x_scroll()
                .map(|scroller| {
                    if own_line {
                        scroller.w_full()
                    } else {
                        scroller
                            .flex_grow(1.0)
                            .flex_basis(px(0.0))
                            .min_w(px(PILLS_MIN_BESIDE_WORD * s))
                    }
                })
                .child(row)
                .into_any_element(),
        )
    }

    /// "Previous question" as a ? pill in the open Q-tags' amber outline (CDXC:SessionChat 2026-10-15
    /// on gx-chat-core's `TagState`), while the chat has unanswered questions; a click jumps to the
    /// previous one (open_questions.rs).
    ///
    /// CDXC:SessionChat 2026-10-15 DECISION:
    /// User: "make the Question pill and scroll to bottom pill not floating, make them appear as the leftmost in the pills row and they should both just be icons. (down arrow for scroll to bottom and ? icon for the question button) always leftmost pls". Both are icon pills at the left end of the working row's pills, with their name in a tooltip; they keep what they did. Then: "make the scroll to bottom appear left most so it doesn't push stuff when it appears", so scroll-to-bottom comes first, then the question pill; the row is right-aligned, so the one that comes and goes most sits where it moves nothing. This supersedes their floating spot over the transcript's bottom edge (2026-09-23, 2026-10-14), which the Escape and "Agent was interrupted" toasts keep on their own (scroll_bottom.rs). The phone does the same in ChatPills.tsx.
    fn previous_question_icon_pill(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        self.open_question_pill_text()?;
        let (color, border) = super::open_questions::question_pill_tone(p.light);
        let label = text(&self.snapshot["openQuestions"], "accessibilityLabel");
        Some(
            icon_pill(
                "chat-previous-question",
                "titlebar/question-mark.svg",
                color,
                p,
            )
            .aria_label(label)
            .border_color(border)
            .hover(move |style| style.bg(color.opacity(0.08)))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Previous question").build(window, cx)
            })
            .on_click(cx.listener(|chat, _, _, cx| chat.previous_question(cx)))
            .into_any_element(),
        )
    }

    /// "Scroll to bottom" as a down-arrow pill, while the reader is away from the bottom; its tooltip
    /// names it and its shortcut (scroll_bottom.rs).
    fn scroll_bottom_icon_pill(
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
            .on_click(cx.listener(|chat, _, _, cx| chat.jump_to_bottom(cx)))
            .into_any_element(),
        )
    }

    /// One pill in the Worked row's chip look (completed_work_row.rs): icon, label, count.
    fn chat_pill(&self, pill: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let id = text(pill, "id");
        let trigger = trigger_id(&id);
        let open = self.chat_menu_is_open(trigger);
        let bounds = self.pill_bounds.cell(&id);
        let measured = bounds.clone();
        let count = text(pill, "count");
        div()
            .id(trigger)
            .relative()
            .role(gpui::Role::Button)
            .aria_label(text(pill, "ariaLabel"))
            .aria_expanded(open)
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
            .chat_cursor_pointer()
            .when(open, |pill| pill.bg(p.foreground.opacity(0.08)))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
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
            .on_click(cx.listener(move |this, _, window, cx| {
                this.toggle_pill_menu(&id, bounds.get(), window, cx)
            }))
            .child(
                canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .into_any_element()
    }

    /// Opens `id`'s menu against its pill, or shuts it when it is the one on screen.
    fn toggle_pill_menu(
        &mut self,
        id: &str,
        trigger: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trigger_id = trigger_id(id);
        if self.chat_menu_toggled_shut(trigger_id, cx) {
            return;
        }
        let Some(height) = self.pill_menu_height(id) else {
            return;
        };
        self.menu_above = true;
        self.show_chat_menu(
            vec![json!({"pillMenu": {"id": id, "height": height}})],
            trigger,
            menu_width(id),
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(trigger_id));
        }
    }

    /// The menu's content height (unscaled) for `id`, or `None` when its panel is gone. The Files
    /// and links menu is capped at 60% of the pane; its list scrolls inside.
    pub(super) fn pill_menu_height(&self, id: &str) -> Option<f32> {
        if id == "filesLinks" {
            let scale = ChatAppearance::current(&self.snapshot).scale;
            let pane = f32::from(self.bounds.get().size.height) / scale;
            return super::files_links::files_links_menu_height(
                &self.snapshot["filesLinks"],
                (pane * 0.6).max(160.0),
            );
        }
        panel_menu_height(id, &self.snapshot)
    }

    /// The body `id`'s menu draws, built with this view's listeners so its rows act on this chat;
    /// `window` is the menu's own.
    pub(super) fn pill_menu_body(
        &mut self,
        id: &str,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match id {
            "filesLinks" => self.files_links_menu_body(p, window, cx),
            "threads" => self.threads_menu_body(p, cx),
            "tasks" => self.tasks_menu_body(p, cx),
            "subagents" => self.fleet_menu_body(p, cx),
            "background" => self.background_menu_body(p),
            _ => None,
        }
    }

    /// Closes the open pill menu, after a row that leaves the chat (a thread opened).
    pub(super) fn close_pill_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.option_menu.clone() {
            menu.update(cx, |menu, cx| menu.close(None, cx));
        }
    }

    /// A pill's menu went away. The Threads list goes back to its first step, as folding the panel
    /// did (CDXC:Coordinators 2026-10-15 in gx-chat-core's coordinator_threads.rs).
    pub(super) fn pill_menu_closed(&mut self, trigger: &'static str, cx: &mut Context<Self>) {
        if trigger == "chat-pill-threads" {
            self.invoke(json!({"type":"toggleCoordinatorThreadsMore","step":0}), cx);
        }
        if trigger == super::files_links::FILES_LINKS_TRIGGER {
            self.files_links_menu_closed(cx);
        }
    }
}
