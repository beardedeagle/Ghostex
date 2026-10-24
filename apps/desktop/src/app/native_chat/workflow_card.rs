//! The workflow card: a Claude Code Workflow call drawn as the run it started, in the shell every
//! transcript card wears (`SINGLE_LINE_CARD`, cards.rs). A header with the run's state, name,
//! description and agent counts; one small heading per phase with its progress; one row per agent
//! (the agent's logo in the card tile with a status badge, its label, a one-line preview, its
//! duration and a chevron) that opens the agent in the subagent viewer; then the script in the
//! transcript's code block, clipped until "Show full script".
//!
//! Everything it says comes from the core's `workflowCards`
//! (packages/gx-chat-core/src/transcript/workflow_card.rs, where the user's request is the
//! `CDXC:SessionChat 2026-10-22 DECISION`); this file only lays it out. The phone draws the same
//! card in apps/mobile/app/src/chat/native/transcript/WorkflowCard.tsx.

use super::cards::{SINGLE_LINE_CARD, card_hover_fill};
use super::new_session_welcome::brand_logo_color;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::workspace_tab_agent_icon_path;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontFeatures, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde_json::{Value, json};

/// The script block's header and one code line, unscaled, for the collapsed clip.
const SCRIPT_HEADER: f32 = 40.0;
const SCRIPT_LINE: f32 = 20.0;

fn hue(p: &ChatAppearance, dark: u32, light: u32) -> Hsla {
    rgb(if p.light { light } else { dark }).into()
}

impl NativeChatView {
    /// The card of every Workflow call `ids` names that the document has a run for.
    pub(super) fn workflow_cards(
        &self,
        ids: &[Value],
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let cards: Vec<AnyElement> = ids
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|id| {
                let card = &self.snapshot["workflowCards"][id];
                card.is_object()
                    .then(|| self.workflow_card(id, card, p, cx))
            })
            .collect();
        (!cards.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap(px(8.0 * p.scale))
                .children(cards)
                .into_any_element()
        })
    }

    /// Whether a tool row is drawn as its workflow card instead.
    pub(super) fn has_workflow_card(&self, tool: &Value) -> bool {
        tool["workflow"]
            .as_str()
            .is_some_and(|id| self.snapshot["workflowCards"][id].is_object())
    }

    fn workflow_card(
        &self,
        id: &str,
        card: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let shape = &*SINGLE_LINE_CARD;
        let hairline = p.foreground.opacity(if p.light { 0.08 } else { 0.05 });
        let header = div()
            .flex()
            .items_start()
            .gap(px(10.0 * s))
            .px(px(shape.padding_x * s))
            .pt(px(10.0 * s))
            .min_w_0()
            .child(
                shape.tile_box(p).child(
                    gpui::svg()
                        .path("titlebar/sitemap.svg")
                        .size(px(shape.glyph_size() * s))
                        .text_color(p.prose),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0 * s))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0 * s))
                            .min_w_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.5 * s))
                                    .text_color(p.card_muted)
                                    .child(text(card, "heading")),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.5 * s))
                                    .text_color(p.card_muted)
                                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                                    .child(text(card, "summary")),
                            ),
                    )
                    .child(
                        shape
                            .title(div().min_w_0().truncate(), p)
                            .text_color(p.foreground)
                            .child(text(card, "name")),
                    )
                    .when(!text(card, "description").is_empty(), |column| {
                        column.child(
                            div()
                                .text_size(px(13.0 * s))
                                .text_color(p.card_muted)
                                .child(text(card, "description")),
                        )
                    }),
            );
        let mut body = div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(10.0 * s))
            .px(px(shape.padding_x * s))
            .pt(px(10.0 * s))
            .pb(px(10.0 * s));
        for phase in card["phases"].as_array().into_iter().flatten() {
            body = body.child(self.workflow_phase(id, phase, p, cx));
        }
        let script = self.workflow_script(id, card, hairline, p, cx);
        shape
            .shell(div(), p)
            .id(SharedString::from(format!("workflow:{id}")))
            .role(gpui::Role::Group)
            .aria_label(format!("{}: {}", text(card, "heading"), text(card, "name")))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .child(header)
            .child(body)
            .children(script)
            .into_any_element()
    }

    fn workflow_phase(
        &self,
        id: &str,
        phase: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let title = text(phase, "title");
        let mut column = div().flex().flex_col().min_w_0().gap(px(2.0 * s));
        if !title.is_empty() {
            let detail = text(phase, "detail");
            column = column.child(
                div()
                    .id(SharedString::from(format!(
                        "workflow:{id}:{}",
                        text(phase, "key")
                    )))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .pb(px(2.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child(div().font_weight(FontWeight::MEDIUM).child(title))
                    .child(
                        div()
                            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                            .child(text(phase, "countLabel")),
                    ),
            );
            // The phase's description from the script, in full under its heading.
            if !detail.is_empty() {
                column = column.child(
                    div()
                        .pb(px(2.0 * s))
                        .text_size(px(12.0 * s))
                        .line_height(px(16.0 * s))
                        .text_color(p.muted)
                        .child(detail),
                );
            }
        }
        for agent in phase["agents"].as_array().into_iter().flatten() {
            column = column.child(self.workflow_agent_row(id, agent, p, cx));
        }
        column.into_any_element()
    }

    fn workflow_agent_row(
        &self,
        id: &str,
        agent: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let shape = &*SINGLE_LINE_CARD;
        let label = text(agent, "label");
        let status = text(agent, "status");
        let mark = shape.glyph_size() * s;
        let logo = match workspace_tab_agent_icon_path("claude") {
            Some(asset) => gpui::svg()
                .path(asset)
                .size(px(mark))
                .text_color(brand_logo_color("claude", p))
                .into_any_element(),
            None => gpui::svg()
                .path("titlebar/robot.svg")
                .size(px(mark))
                .text_color(p.foreground)
                .into_any_element(),
        };
        let badge = match status.as_str() {
            "done" => shape.check_badge(p),
            "failed" => shape.error_badge(p),
            "running" => {
                let blue = hue(p, 0x60a5fa, 0x2563eb);
                shape.badge(blue, "titlebar/circle.svg", blue, p)
            }
            _ => {
                let grey = p.muted.opacity(0.6);
                shape.badge(grey, "titlebar/circle.svg", grey, p)
            }
        };
        let command = NativeChatView::subagent_open_command(&json!({
            "selector": agent["selector"],
            "name": label,
            "agentType": "workflow-subagent",
            // The viewer's subtitle: which of the run's agents this is.
            "task": label,
            "model": agent["model"],
        }));
        let preview = text(agent, "preview");
        let duration = text(agent, "durationLabel");
        let opens = command.is_some();
        div()
            .id(SharedString::from(format!(
                "workflow:{id}:agent:{}",
                text(agent, "key")
            )))
            .role(gpui::Role::Button)
            .aria_label(format!(
                "{label}, {}{}",
                text(agent, "statusLabel"),
                if duration.is_empty() {
                    String::new()
                } else {
                    format!(", {duration}")
                }
            ))
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .min_w_0()
            .mx(px(-6.0 * s))
            .px(px(6.0 * s))
            .py(px(5.0 * s))
            .rounded(px(8.0 * s))
            .when(opens, |row| {
                row.chat_cursor_pointer()
                    .hover(|style| style.bg(card_hover_fill(p)))
            })
            .child(shape.tile_box(p).child(logo).child(badge))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.5 * s))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.foreground)
                            .child(label.clone()),
                    )
                    .when(!preview.is_empty(), |column| {
                        column.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.5 * s))
                                .text_color(p.card_muted)
                                .child(preview),
                        )
                    }),
            )
            .when(!duration.is_empty(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.5 * s))
                        .text_color(p.card_muted)
                        .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                        .child(duration),
                )
            })
            .when(opens, |row| {
                row.child(
                    gpui::svg()
                        .path("titlebar/chevron-right.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
            })
            .when_some(command, |row, command| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.invoke(command.clone(), cx);
                }))
            })
            .into_any_element()
    }

    /// The script in the transcript's code block, clipped to its first lines until the reader
    /// asks for the rest; the block's own copy and wrap buttons work on the whole script.
    fn workflow_script(
        &self,
        id: &str,
        card: &Value,
        hairline: Hsla,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let markdown = text(card, "scriptMarkdown");
        if markdown.is_empty() {
            return None;
        }
        let s = p.scale;
        let key = format!("workflow-script:{id}");
        let collapsible = card["scriptCollapsible"] == true;
        let expanded = !collapsible || self.expanded.contains(&key);
        let lines = card["scriptPreviewLines"].as_u64().unwrap_or(6) as f32;
        let block = self.markdown(key.clone(), markdown, &Value::Null, p, cx);
        let clipped = div()
            .min_w_0()
            .when(!expanded, |clip| {
                clip.max_h(px((SCRIPT_HEADER + lines * SCRIPT_LINE) * s))
                    .overflow_hidden()
            })
            .child(block);
        let toggle = collapsible.then(|| {
            let label = text(
                card,
                if expanded {
                    "hideScriptLabel"
                } else {
                    "showScriptLabel"
                },
            );
            let toggle_key = key.clone();
            let (muted, foreground) = (p.muted, p.foreground);
            div()
                .id(SharedString::from(format!("{key}:toggle")))
                .role(gpui::Role::Button)
                .aria_label(label.clone())
                .chat_cursor_pointer()
                .pt(px(6.0 * s))
                .text_size(px(12.5 * s))
                .text_color(muted)
                .hover(move |style| style.text_color(foreground))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if !this.expanded.remove(&toggle_key) {
                        this.expanded.insert(toggle_key.clone());
                    }
                    cx.notify();
                }))
        });
        Some(
            div()
                .border_t_1()
                .border_color(hairline)
                .px(px(SINGLE_LINE_CARD.padding_x * s))
                .pt(px(10.0 * s))
                .pb(px(10.0 * s))
                .min_w_0()
                .child(clipped)
                .children(toggle)
                .into_any_element(),
        )
    }
}
