//! The working card at the transcript's end (`workingCard`, decided in
//! packages/gx-chat-core/src/extras/working_card.rs): one line with the spark, the label and the
//! pills, which a click opens to the running tool's call and a section per pill. working_lane.rs
//! places it; this draws it.
//!
//! CDXC:SessionChat 2026-10-17 SEE-ALSO:
//! The phone draws the same card in apps/mobile/app/src/chat/native/cards/WorkingCard.tsx.

use super::{
    appearance::ChatAppearance,
    state::NativeChatView,
    transcript::text,
    working_spark::{css_ease_in_out, spark},
};
use crate::app::helpers::ThrottledAnimationExt;
use crate::app::native_chat::cursor::ChatCursor as _;
use crate::assets::chat_working::VISUAL;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontFeatures, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, px, relative, svg,
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc, time::Duration};

/// The disclosure key the card's body opens and closes under.
const CARD_KEY: &str = "working-card";

/// The header line's padding above and below (unscaled).
const HEADER_PAD_Y: f32 = 6.0;

/// What the card measured at its last paint.
#[derive(Default)]
pub(super) struct WorkingLaneCells {
    /// The height the transcript's last row reserves below itself for the card, in window pixels.
    pub(super) reserved: Rc<Cell<f32>>,
    /// The label did not fit its line, so hovering it shows the whole text.
    label_truncated: Rc<Cell<bool>>,
}

impl NativeChatView {
    /// The extra space the transcript's last row keeps under itself for the card.
    pub(super) fn working_lane_reserve(&self) -> f32 {
        self.working_lane.reserved.get()
    }

    /// The card (or, with nothing to show while the reader is scrolled up, the scroll-to-bottom
    /// pill alone) over the transcript, placed by [`super::working_lane::WorkingLane`].
    pub(super) fn render_working_lane(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let card = self.snapshot["workingCard"].clone();
        let armed = self
            .armed_actions
            .as_array()
            .is_some_and(|actions| !actions.is_empty());
        let rows = self.list.item_count();
        let visible = rows > 0 && (card["visible"] == true || armed);
        let left_bottom = rows > 0 && super::scroll_bottom::transcript_left_bottom(&self.list);
        if !visible {
            self.set_lane_reserve(0.0, cx);
        }
        let content = if visible {
            self.working_card(&card, left_bottom, p, cx)
        } else if left_bottom {
            div()
                .flex()
                .justify_end()
                .px(px(VISUAL.padding_x * s))
                .block_mouse_except_scroll()
                .children(self.scroll_bottom_icon_pill(p, cx))
                .into_any_element()
        } else {
            return None;
        };
        // The column the transcript's rows use (transcript.rs), so the card lines up with them.
        let column = div()
            .w_full()
            .max_w(px(768.0 * s))
            .px(px(16.0 * s))
            .when_some(p.transcript_width, |this, width| {
                this.max_w(relative(1.0)).w(relative(width))
            })
            .when_some(self.held_text_width, |this, width| {
                this.flex_shrink_0().w(px(width)).max_w(px(width))
            })
            .child(content);
        let layer = div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .justify_end()
            .pb(px(super::transcript_layout::LAYOUT.end_padding * s))
            .child(div().w_full().flex().justify_center().child(column))
            .into_any_element();
        Some(
            super::working_lane::WorkingLane {
                list: self.list.clone(),
                child: layer,
            }
            .into_any_element(),
        )
    }

    /// Sets the space the last row keeps for the card and has that row measured again.
    fn set_lane_reserve(&self, height: f32, cx: &mut Context<Self>) {
        if self.working_lane.reserved.replace(height) == height {
            return;
        }
        if let Some(last) = self.list.item_count().checked_sub(1) {
            self.list.remeasure_items(last..last + 1);
        }
        if let Some(host) = &self.transcript_host {
            gpui::App::notify(cx, host.entity_id());
        }
    }

    fn working_card(
        &mut self,
        card: &Value,
        left_bottom: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let expandable = card["expandable"] == true;
        let open = expandable && card["open"] == true;
        let motion = self.disclosure_frame(CARD_KEY, open, cx);
        let header = self.working_card_header(card, left_bottom, expandable, open, p, cx);
        let body = (open || motion.is_some())
            .then(|| self.working_card_body(card, p, cx))
            .map(|body| self.disclosure_body_motion(CARD_KEY, motion, 0.0, body));
        // The card's border box, which the transcript's last row reserves below itself.
        let reserved = self.working_lane.reserved.clone();
        let list = self.list.clone();
        let host = self.transcript_host.as_ref().map(|host| host.entity_id());
        let measure = canvas(
            move |bounds, _, cx| {
                let height = f32::from(bounds.size.height);
                if (reserved.get() - height).abs() > 0.5 {
                    reserved.set(height);
                    if let Some(last) = list.item_count().checked_sub(1) {
                        list.remeasure_items(last..last + 1);
                    }
                    if let Some(host) = host {
                        cx.notify(host);
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset(px(-1.0));
        div()
            .id("chat-working-card")
            .relative()
            .role(gpui::Role::Status)
            .aria_label(text(card, "ariaLabel"))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.input_border)
            .rounded(px(12.0 * s))
            .bg(p.card_panel)
            .overflow_hidden()
            .text_color(p.card_muted)
            // Over the transcript, a press or a hover on the card is the card's; the wheel still
            // scrolls the transcript under it.
            .block_mouse_except_scroll()
            .child(header)
            .children(body)
            .child(measure)
            .into_any_element()
    }

    /// The card's line: the lead glyph and the label on the left, cut with an ellipsis where it
    /// meets the pills, which keep their full width on the right.
    fn working_card_header(
        &self,
        card: &Value,
        left_bottom: bool,
        expandable: bool,
        open: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let reduced_motion = crate::app::helpers::gpui_macos_reduce_motion_enabled();
        let kind = card["labelKind"].as_str().unwrap_or_default();
        let activity = &card["activity"];
        let lead = match kind {
            "" => None,
            "activity" => Some(activity_lead(activity, p, reduced_motion)),
            "worked" => Some(
                div()
                    .size(px(VISUAL.spark_box * s))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .path("titlebar/check.svg")
                            .size(px(VISUAL.spark_size * s))
                            .text_color(p.muted),
                    )
                    .into_any_element(),
            ),
            _ if card["working"] == true => Some(spark(p, reduced_motion)),
            _ => None,
        };
        let label = card["label"].as_str().map(str::to_owned);
        let chat = cx.entity_id();
        let label = label.map(|label| self.working_card_label(label, kind, chat, p));
        let mut trailing = Vec::new();
        if kind == "activity" {
            if let Some(hint) = activity["hint"].as_str() {
                let hint = hint.to_owned();
                trailing.push(
                    div()
                        .id("compaction-messaging-hint")
                        .role(gpui::Role::Button)
                        .aria_label("Messaging during compaction")
                        .size(px(20.0 * s))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
                        })
                        .child(
                            svg()
                                .path("titlebar/info-circle.svg")
                                .size(px(14.0 * s))
                                .text_color(p.muted),
                        )
                        .into_any_element(),
                );
            }
            trailing.extend(activity_trailing(activity, p, reduced_motion));
        }
        // Right to left from the card's edge: the panel pills, the question pill, the armed timers,
        // and scroll-to-bottom at the far left, so one appearing moves none of the others; the
        // label gives up the room.
        let pills = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(6.0 * s))
            .children(
                left_bottom
                    .then(|| self.scroll_bottom_icon_pill(p, cx))
                    .flatten(),
            )
            .children(self.armed_action_items(p, cx))
            .children(self.previous_question_icon_pill(p, cx))
            .children(
                card["sections"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter_map(|id| {
                        self.snapshot["chatPills"]
                            .as_array()?
                            .iter()
                            .find(|pill| pill["id"] == id)
                    })
                    .map(|pill| super::chat_pills::chat_pill(pill, p)),
            );
        let hover = super::cards::card_hover_fill(p);
        div()
            .id("chat-working-card-header")
            .role(gpui::Role::Button)
            .aria_expanded(open)
            .w_full()
            .min_w_0()
            .min_h(px(VISUAL.min_height * s))
            .pl(px(12.0 * s))
            .pr(px(10.0 * s))
            .pt(px(HEADER_PAD_Y * s))
            .pb(px(HEADER_PAD_Y * s))
            .flex()
            .items_center()
            .gap(px(VISUAL.gap * s))
            .when(expandable, |header| {
                header
                    .chat_cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        chat.invoke(json!({"type":"toggleWorkingCard","open":!open}), cx);
                    }))
            })
            .children(lead)
            .child(
                // Takes what the pills leave; empty when the card carries only pills.
                div()
                    .flex()
                    .items_center()
                    .gap(px(VISUAL.gap * s))
                    .flex_1()
                    .min_w_0()
                    .children(label)
                    .children(trailing),
            )
            .child(pills)
            .into_any_element()
    }

    /// The label on one line, cut with an ellipsis at its end; when it is cut, hovering it shows the
    /// whole text in the app's tooltip.
    fn working_card_label(
        &self,
        label: String,
        kind: &str,
        chat: gpui::EntityId,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let truncated = self.working_lane.label_truncated.clone();
        let cut = truncated.get();
        let measured = label.clone();
        let full = label.clone();
        let (size, color) = match kind {
            "tool" | "activity" => (13.0, p.foreground),
            _ => (VISUAL.font_size, p.muted),
        };
        div()
            .id("chat-working-card-label")
            .relative()
            .min_w_0()
            .flex_shrink(1.0)
            .text_size(px(size * s))
            .line_height(relative(1.5))
            .text_color(color)
            .truncate()
            .when(cut, |this| {
                this.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(full.clone()).build(window, cx)
                })
            })
            .child(label)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let style = window.text_style();
                        let font_size = style.font_size.to_pixels(window.rem_size());
                        let run = style.to_run(measured.len());
                        let width = window
                            .text_system()
                            .shape_line(measured.clone().into(), font_size, &[run], None)
                            .width;
                        let cut = width > bounds.size.width + px(0.5);
                        if truncated.replace(cut) != cut {
                            cx.notify(chat);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    /// The open card: the running tool's call, then one section per pill, each drawn with the rows
    /// its pill's menu used to show.
    fn working_card_body(
        &mut self,
        card: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut sections: Vec<AnyElement> = Vec::new();
        let tool = &card["tool"];
        if tool.is_object() {
            let header = super::chat_pills::pill_menu_header(
                "titlebar/terminal-2.svg",
                "Running now",
                text(tool, "label"),
                None,
                p,
            );
            let detail = div()
                .id("chat-working-card-tool-detail")
                .max_h(px(200.0 * s))
                .overflow_y_scroll()
                .p(px(10.0 * s))
                .bg(p.input)
                .rounded(px(6.0 * s))
                .child(self.markdown(
                    "working-card-tool".to_string(),
                    format!("```\n{}\n```", text(tool, "detail")),
                    &Value::Null,
                    p,
                    cx,
                ))
                .into_any_element();
            sections.push(super::chat_pills::pill_menu_column(
                "chat-working-card-tool",
                "Running now",
                header,
                Some(detail),
                None,
                p,
            ));
        }
        for id in card["sections"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let section = match id {
                "threads" => self.threads_menu_body(p, cx),
                "tasks" => self.tasks_menu_body(p, cx),
                "subagents" => self.fleet_menu_body(p, cx),
                "background" => self.background_menu_body(p),
                _ => None,
            };
            sections.extend(section);
        }
        let pane = f32::from(self.bounds.get().size.height);
        div()
            .id("chat-working-card-body")
            .w_full()
            .min_w_0()
            .max_h(px((pane * 0.5).max(160.0 * s)))
            .overflow_y_scroll()
            .px(px(8.0 * s))
            .pt(px(2.0 * s))
            .pb(px(10.0 * s))
            .flex()
            .flex_col()
            .gap(px(12.0 * s))
            .children(sections)
            .into_any_element()
    }

    /// CDXC:DelayedSend 2026-09-21 DECISION:
    /// User: clicking an armed Delayed Send or Close After Done indicator on the chat working row opens the Delayed Actions modal so they can be managed there. (2026-10-17: they ride on the working card, Q259.)
    fn armed_action_items(&self, p: &ChatAppearance, cx: &Context<Self>) -> Vec<AnyElement> {
        let s = p.scale;
        self.armed_actions
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|action| {
                let color = VISUAL.armed_color(action["id"].as_str()?)?;
                let label = action["label"].as_str()?.to_owned();
                let id = action["id"].as_str()?.to_owned();
                Some(
                    div()
                        .id(format!("chat-armed-{id}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("{label}. Manage delayed actions"))
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.8))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.host(
                                "delayedActions",
                                serde_json::json!({"type": "host", "action": "delayedActions"}),
                                cx,
                            );
                        }))
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .gap(px(VISUAL.armed_icon_gap * s))
                        .child(
                            div()
                                .size(px(VISUAL.spark_box * s))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    svg()
                                        .path("titlebar/clock.svg")
                                        .size(px(VISUAL.spark_size * s))
                                        .text_color(color),
                                ),
                        )
                        .child(
                            div()
                                .whitespace_nowrap()
                                .text_size(px(VISUAL.font_size * s))
                                .line_height(relative(1.5))
                                .text_color(p.foreground)
                                .child(label),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}

/// A live activity's lead: the loader while shells run, else the pulsing dot.
fn activity_lead(activity: &Value, p: &ChatAppearance, reduced_motion: bool) -> AnyElement {
    let s = p.scale;
    let glyph = if activity["shellsRunning"] == true {
        let glyph = svg()
            .path("titlebar/loader2.svg")
            .size(px(14.0 * s))
            .text_color(p.control_primary);
        if reduced_motion {
            glyph.into_any_element()
        } else {
            glyph
                .with_throttled_animation(
                    "chat-shells-running",
                    Duration::from_secs(1),
                    |glyph, progress| {
                        glyph.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                            progress,
                        )))
                    },
                )
                .into_any_element()
        }
    } else {
        let dot = div().size(px(6.0 * s)).rounded_full().bg(p.control_primary);
        if reduced_motion {
            dot.into_any_element()
        } else {
            dot.with_throttled_animation(
                "chat-activity-dot",
                Duration::from_millis(1600),
                |dot, phase| {
                    let progress = css_ease_in_out(if phase < 0.5 {
                        phase * 2.0
                    } else {
                        (1.0 - phase) * 2.0
                    });
                    dot.opacity(1.0 - 0.65 * progress)
                },
            )
            .into_any_element()
        }
    };
    div()
        .size(px(VISUAL.spark_box * s))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
        .into_any_element()
}

/// A live activity's clock, tokens, percent and a small progress track after its label.
fn activity_trailing(
    activity: &Value,
    p: &ChatAppearance,
    reduced_motion: bool,
) -> Vec<AnyElement> {
    let s = p.scale;
    let numbers = |value: String| {
        div()
            .flex_shrink_0()
            .text_size(px(VISUAL.font_size * s))
            .text_color(p.muted)
            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
            .child(value)
            .into_any_element()
    };
    let mut parts = Vec::new();
    let percent = activity["percent"].as_f64();
    if percent.is_some() || activity["indeterminate"] == true {
        let primary = p.control_primary;
        let track = div()
            .relative()
            .w(px(48.0 * s))
            .h(px(4.0 * s))
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden()
            .bg(p.foreground.opacity(0.1));
        let fill = div().h_full().rounded_full().bg(primary);
        parts.push(if let Some(percent) = percent {
            track
                .child(fill.w(relative(percent as f32 / 100.0)))
                .into_any_element()
        } else if reduced_motion {
            track
                .flex()
                .justify_center()
                .child(fill.w(relative(0.35)))
                .into_any_element()
        } else {
            track
                .with_throttled_animation(
                    "chat-compaction-progress",
                    Duration::from_millis(1800),
                    move |track, phase| {
                        track.child(
                            div()
                                .absolute()
                                .h_full()
                                .rounded_full()
                                .bg(primary)
                                .w(relative(0.35))
                                .left(relative(-0.35 + 1.351 * css_ease_in_out(phase))),
                        )
                    },
                )
                .into_any_element()
        });
    }
    if let Some(elapsed) = activity["elapsedLabel"].as_str() {
        parts.push(numbers(elapsed.to_owned()));
    }
    if let Some(tokens) = activity["tokens"].as_str() {
        parts.push(numbers(tokens.to_owned()));
    }
    if let Some(percent) = percent {
        parts.push(numbers(format!("{percent}%")));
    }
    parts
}
