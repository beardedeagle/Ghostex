//! The cards for messages between agents: one another agent sent this session, one this session's
//! agent sent another, and a subagent's reply. Each is one 44px row (the agent's logo, one sentence
//! such as `Message from "<session>"`, the time and the status as a pill) that opens to the whole
//! message; a message
//! sent to several sessions at once lists each of them above it.
//! Which rows these are, whom a message went to, and everything a card says comes from the core's
//! `card` object (packages/gx-chat-core/src/transcript/agent_message_card.rs, where the user's pick
//! is the `CDXC:SessionChat 2026-10-12 DECISION`); this file only lays it out, in the shell, height
//! and title type every one-line card shares (`SINGLE_LINE_CARD`, cards.rs).
//! The phone draws the same cards in apps/mobile/app/src/chat/native/transcript/AgentMessageCards.tsx.

use super::cards::{SINGLE_LINE_CARD, card_hover_fill};
use super::new_session_welcome::brand_logo_color;
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::helpers::workspace_tab_agent_icon_path;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, img, px, rgb,
};
use serde_json::{Value, json};

fn hue(p: &ChatAppearance, dark: u32, light: u32) -> Hsla {
    rgb(if p.light { light } else { dark }).into()
}

fn tone_color(tone: &str, p: &ChatAppearance) -> Hsla {
    match tone {
        "failed" => hue(p, 0xfb7185, 0xe11d48),
        "waiting" => hue(p, 0xfbbf24, 0xd97706),
        _ => hue(p, 0x34d399, 0x059669),
    }
}

fn icon(path: &'static str, size: f32, color: Hsla) -> AnyElement {
    gpui::svg()
        .path(path)
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
        .into_any_element()
}

/// What one card draws: its key, the core's `card`, and the message body with its references.
struct AgentCard<'a> {
    key: String,
    card: &'a Value,
    body: String,
    references: &'a Value,
}

impl NativeChatView {
    /// A message another agent sent this session.
    ///
    /// CDXC:SessionChat 2026-09-18 DECISION:
    /// User: a message another agent sent with `ghostex agents send` reads as a message from that agent, not as the user's own prompt bubble, and its sender header is never a heading.
    pub(super) fn inter_agent_message_card(
        &self,
        id: &str,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let inter = &message["interAgentMessage"];
        self.agent_card(
            AgentCard {
                key: format!("inter-agent:{id}"),
                card: &inter["card"],
                body: text(inter, "body"),
                references: &message["markdownReferences"],
            },
            p,
            cx,
        )
    }

    /// A subagent's reply that arrived as a system turn (`Message from <agent>`).
    pub(super) fn subagent_message_card(
        &self,
        id: &str,
        card: &Value,
        references: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let marked = text(card, "markdown");
        self.agent_card(
            AgentCard {
                key: format!("agent-message:{id}"),
                card: &card["card"],
                body: if marked.is_empty() {
                    text(card, "body")
                } else {
                    marked
                },
                references,
            },
            p,
            cx,
        )
    }

    /// The cards for the messages one message's tool calls sent, or a finished turn lifted out of
    /// its work fold.
    ///
    /// CDXC:SessionChat 2026-09-30 DECISION:
    /// User: a message this session's agent sends another agent gets "a small card that shows the message being sent when expanded", "collapsed by default", in the received card's shape.
    pub(super) fn sent_agent_message_cards(
        &self,
        cards: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let cards = cards.as_array().filter(|cards| !cards.is_empty())?;
        Some(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .w_full()
                .gap(px(8.0 * p.scale))
                .children(cards.iter().map(|sent| {
                    let marked = text(sent, "markdown");
                    self.agent_card(
                        AgentCard {
                            key: format!("sent-message:{}", text(sent, "key")),
                            card: &sent["card"],
                            body: if marked.is_empty() {
                                text(sent, "body")
                            } else {
                                marked
                            },
                            references: &sent["markdownReferences"],
                        },
                        p,
                        cx,
                    )
                }))
                .into_any_element(),
        )
    }

    /// One card: the row, the whole message under it once opened, and why a message did not go out.
    fn agent_card(
        &self,
        input: AgentCard<'_>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let AgentCard {
            key,
            card,
            body,
            references,
        } = input;
        let expandable = !body.is_empty();
        let expanded = expandable && self.expanded.contains(&key);
        let motion = self.disclosure_frame(&key, expanded, cx);
        let hairline = p.foreground.opacity(if p.light { 0.08 } else { 0.05 });
        let opened = (expanded || motion.is_some()).then(|| {
            let message = div()
                .border_t_1()
                .border_color(hairline)
                .px(px(12.0 * s))
                .pt(px(8.0 * s))
                .pb(px(10.0 * s))
                .min_w_0()
                .text_color(p.card_muted)
                .children(self.agent_card_recipients(&key, card, p, cx))
                .child(self.markdown(key.clone(), body, references, p, cx))
                .into_any_element();
            self.disclosure_body_motion(&key, motion, 0.0, message)
        });
        let failure = self.agent_card_failure(&key, card, hairline, p, cx);
        SINGLE_LINE_CARD
            .shell(div(), p)
            .group(SharedString::from(format!("agent-card:{key}")))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .child(self.agent_card_row(&key, card, expandable, expanded, p, cx))
            .children(opened)
            .children(failure)
            .into_any_element()
    }

    /// The logo, the core's sentence (the same collapsed and opened), then Go to session on hover,
    /// the time and the status pill; no chevron. The row opens and closes the message.
    fn agent_card_row(
        &self,
        key: &str,
        card: &Value,
        expandable: bool,
        expanded: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let session = &card["session"];
        let status = &card["status"];
        let time = text(card, "time");
        let group = SharedString::from(format!("agent-card:{key}"));
        let shape = &*SINGLE_LINE_CARD;
        let row = div()
            .id(SharedString::from(format!("{key}:row")))
            .h(px(shape.header_height(s)))
            .flex_shrink_0()
            .px(px(shape.padding_x * s))
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .min_w_0()
            .child(self.agent_card_mark(card, p))
            .child(
                shape
                    .title(div().flex_1().min_w_0().truncate(), p)
                    .text_color(p.foreground)
                    .child(text(card, "name")),
            )
            .when(
                session.is_object() && text(card, "failure").is_empty(),
                |row| {
                    let command = json!({
                        "type": "openAgentSession",
                        "projectId": session["projectId"],
                        "sessionId": session["sessionId"],
                    });
                    let (muted, foreground, fill) =
                        (p.muted, p.foreground, p.foreground.opacity(0.08));
                    row.child(
                        div()
                            .id(SharedString::from(format!("{key}:session")))
                            .invisible()
                            .group_hover(group.clone(), |style| style.visible())
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(4.0 * s))
                            .px(px(6.0 * s))
                            .py(px(1.0 * s))
                            .rounded(px(6.0 * s))
                            .text_size(px(12.5 * s))
                            .text_color(muted)
                            .whitespace_nowrap()
                            .role(gpui::Role::Button)
                            .aria_label("Go to session")
                            .chat_cursor_pointer()
                            .hover(move |style| style.text_color(foreground).bg(fill))
                            .child(icon("titlebar/arrow-up-right.svg", 12.0 * s, muted))
                            .child("Go to session")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.invoke(command.clone(), cx)
                            })),
                    )
                },
            )
            .when(!time.is_empty(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(time),
                )
            })
            .when(status.is_object(), |row| {
                row.child(self.agent_card_status(key, status, p))
            });
        if !expandable {
            return row.into_any_element();
        }
        let key = key.to_string();
        let hover = card_hover_fill(p);
        row.role(gpui::Role::Button)
            .aria_label(text(card, "title"))
            .aria_expanded(expanded)
            .chat_cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_marker_disclosure(key.clone(), expanded, cx)
            }))
            .into_any_element()
    }

    /// A message sent to several sessions lists each one above the message, with its own status;
    /// a recipient's name opens its session.
    fn agent_card_recipients(
        &self,
        key: &str,
        card: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let recipients = card["recipients"]
            .as_array()
            .filter(|recipients| !recipients.is_empty())?;
        let s = p.scale;
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .pb(px(8.0 * s))
                .text_size(px(13.0 * s))
                .children(recipients.iter().enumerate().map(|(index, recipient)| {
                    let row_key = format!("{key}:recipient:{index}");
                    let session = &recipient["session"];
                    let name = div()
                        .id(SharedString::from(format!("{row_key}:name")))
                        .min_w_0()
                        .truncate()
                        .text_color(p.foreground)
                        .child(text(recipient, "name"))
                        .when(session.is_object(), |name| {
                            let command = json!({
                                "type": "openAgentSession",
                                "projectId": session["projectId"],
                                "sessionId": session["sessionId"],
                            });
                            name.role(gpui::Role::Button)
                                .aria_label("Go to session")
                                .chat_cursor_pointer()
                                .hover(|style| style.underline())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.invoke(command.clone(), cx)
                                }))
                        });
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0 * s))
                        .min_w_0()
                        .child(self.agent_card_mark(recipient, p))
                        .child(name)
                        .when(recipient["status"].is_object(), |row| {
                            row.child(self.agent_card_status(&row_key, &recipient["status"], p))
                        })
                }))
                .into_any_element(),
        )
    }

    /// A pill (`SINGLE_LINE_CARD`, cards.rs): Delivered is a green check; Queued, Waiting for agent
    /// and Not sent say so in their colour.
    fn agent_card_status(&self, key: &str, status: &Value, p: &ChatAppearance) -> AnyElement {
        let color = tone_color(&text(status, "tone"), p);
        let glyph = match text(status, "icon").as_str() {
            "alert" => "titlebar/alert-triangle.svg",
            "clock" => "titlebar/clock.svg",
            _ => "titlebar/check.svg",
        };
        let hint = SharedString::from(text(status, "hint"));
        let icon_only = status["iconOnly"] == true;
        SINGLE_LINE_CARD
            .pill(
                Some(glyph),
                (!icon_only).then(|| text(status, "label")),
                color,
                p,
            )
            .id(SharedString::from(format!("{key}:status")))
            .role(gpui::Role::Status)
            .aria_label(hint.clone())
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
            })
            .into_any_element()
    }

    /// The agent's own logo (a coordinator's crown, `CDXC:Coordinators 2026-10-15` in the core's
    /// agent_message_card.rs) with a small arrow badge (prose grey coming in, sky going out), or
    /// the sparkle of a subagent, the size of the fold's tile and wearing its badge
    /// (`SINGLE_LINE_CARD`, cards.rs) so the left icons line up.
    fn agent_card_mark(&self, card: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        let shape = &*SINGLE_LINE_CARD;
        // The mark is the size of the fold's and the question cards' tiles, so the left icons line up.
        let mark = shape.icon_size() * s;
        let direction = text(card, "direction");
        if direction == "subagent" {
            return icon("titlebar/sparkles.svg", mark, p.prose);
        }
        let (badge, arrow) = if direction == "sent" {
            (hue(p, 0x7dd3fc, 0x0284c7), "titlebar/arrow-up-right.svg")
        } else {
            (p.prose, "titlebar/arrow-down-left.svg")
        };
        // CDXC:SessionChat 2026-10-14 WHY: the arrow used to take `card_panel`, which under window glass is a 6% white wash, so it painted nothing on the badge and the row showed a plain grey dot ("it's supposed to be an arrow or graphic in that circle"). The badge fill is opaque and light in dark mode (dark in light mode), so its arrow is a fixed opposite ink instead of a theme wash.
        let arrow_ink = if p.light {
            gpui::Hsla::from(rgb(0xffffff))
        } else {
            gpui::Hsla::from(rgb(0x161616))
        };
        let agent = text(card, "agentIcon");
        let logo = match workspace_tab_agent_icon_path(&agent) {
            // The sidebar's coordinator crown and colours (`coordinator_icon` in native_sidebar/threads.rs).
            _ if card["coordinator"] == true => icon(
                "titlebar/coordinator-crown.svg",
                mark,
                if p.light {
                    rgb(0x000000).into()
                } else {
                    rgb(0xffffff).into()
                },
            ),
            // OMP's artwork is deliberately multicolour (`new_session_agent_mark`).
            Some(asset) if agent == "omp" => img(asset).size(px(mark)).into_any_element(),
            Some(asset) => gpui::svg()
                .path(asset)
                .size(px(mark))
                .text_color(brand_logo_color(&agent, p))
                .into_any_element(),
            None => icon("titlebar/robot.svg", mark, p.foreground),
        };
        div()
            .relative()
            .size(px(mark))
            .flex_shrink_0()
            .child(logo)
            .child(shape.badge(badge, arrow, arrow_ink, p))
            .into_any_element()
    }

    /// A message that did not go out: why, with Retry and Remove when it is this chat's own send.
    fn agent_card_failure(
        &self,
        key: &str,
        card: &Value,
        hairline: Hsla,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let failure = text(card, "failure");
        if failure.is_empty() {
            return None;
        }
        let s = p.scale;
        let retry = card["retryPromptId"].clone();
        let action = |suffix: &str, label: &'static str, command: Value| {
            div()
                .id(SharedString::from(format!("{key}:{suffix}")))
                .role(gpui::Role::Button)
                .aria_label(label)
                .chat_cursor_pointer()
                .px(px(6.0 * s))
                .py(px(2.0 * s))
                .rounded(px(5.0 * s))
                .chat_filled(p.neutral_button_fills(), true)
                .text_color(p.primary)
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
        };
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0 * s))
                .border_t_1()
                .border_color(hairline)
                .px(px(12.0 * s))
                .py(px(8.0 * s))
                .text_size(px(12.0 * s))
                .child(div().flex_1().min_w_0().text_color(p.muted).child(failure))
                .when(retry.is_string(), |row| {
                    row.child(action(
                        "retry",
                        "Retry",
                        json!({"type":"retryQueue","promptId":retry.clone()}),
                    ))
                    .child(action(
                        "remove",
                        "Remove",
                        json!({"type":"removeQueue","promptId":retry.clone()}),
                    ))
                })
                .into_any_element(),
        )
    }
}
