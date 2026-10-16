use super::ask_card::{
    AskBodyMotion, AskOption, AskTone, ask_head, chevron, press_head, progress_dots,
};
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use serde_json::json;

impl NativeChatView {
    pub(crate) fn render_prompt(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.snapshot["questionCard"]["visible"] != true {
            self.collapsed.retain(|key| !key.starts_with("question:"));
            self.answer_input = None;
            self.answer_subscription = None;
            return None;
        }
        if self.snapshot["questionCard"]["loading"] == true {
            return Some(self.status_card(
                "Question".into(),
                "titlebar/help-circle.svg",
                vec![div().child("Restoring your answer…").into_any_element()],
                vec![],
                p,
            ));
        }
        let prompt = self.snapshot["prompt"].clone();
        let s = p.scale;
        let busy = self.snapshot["questionCard"]["busy"] == true;
        if prompt["kind"] == "approval" {
            return Some(self.render_approval(&prompt, p, cx));
        }
        let index = self.snapshot["questionCard"]["questionIndex"]
            .as_u64()
            .unwrap_or(0) as usize;
        let count = prompt["questions"].as_array().map(Vec::len).unwrap_or(0);
        let question = &prompt["questions"][index];
        let draft = self.snapshot["questionCard"]["drafts"][index].clone();
        let header = text(question, "header");
        let collapse_key = format!("question:{}", prompt);
        let collapsed = self.collapsed.contains(&collapse_key);
        let motion = self.disclosure_frame(&collapse_key, !collapsed, cx);
        let multiple = question["multiSelect"] == true;
        let mut options = div()
            .id("question-options")
            .flex()
            .flex_col()
            .gap(px(8.0 * s))
            .max_h(window.viewport_size().height * 0.45)
            .overflow_y_scroll();
        for (option_index, option) in question["options"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            // A multi-select note is sent beside the picks, so they stay visible while it is typed.
            let selected = (multiple || text(&draft, "other").trim().is_empty())
                && draft["indices"].as_array().is_some_and(|indices| {
                    indices
                        .iter()
                        .any(|i| i.as_u64() == Some(option_index as u64))
                });
            options = options.child(self.ask_option(
                AskOption {
                    id: format!("question-option:{index}:{option_index}").into(),
                    label: text(option, "label"),
                    detail: text(option, "description"),
                    picked: selected,
                    multiple,
                    key: (option_index < 9).then(|| (option_index + 1).to_string()),
                    recommended: false,
                    other: false,
                    enabled: !busy,
                    answered: false,
                    action: json!({"type":"questionOption","index":option_index}),
                },
                p,
                cx,
            ));
            // The mockup Claude shows beside the option, under the row that picks it.
            let preview = text(option, "preview");
            if !preview.is_empty() {
                options = options.child(
                    div()
                        .id(format!("question-option-preview:{index}:{option_index}"))
                        .ml(px(28.0 * s))
                        .max_h(px(160.0 * s))
                        .overflow_y_scroll()
                        .px(px(10.0 * s))
                        .py(px(8.0 * s))
                        .rounded(px(8.0 * s))
                        .bg(p.card_row)
                        .font_family(super::fonts::CHAT_MONO)
                        .text_size(px(12.6 * s))
                        .line_height(px(18.0 * s))
                        .text_color(p.muted)
                        .child(preview),
                );
            }
        }
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.0 * s))
            .child(
                div()
                    .min_w_0()
                    .text_size(px(15.0 * s))
                    .line_height(px(22.0 * s))
                    .text_color(p.foreground)
                    .child(text(question, "question")),
            )
            .when(multiple, |body| {
                body.child(
                    div()
                        .mt(px(-6.0 * s))
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child("Select one or more options."),
                )
            })
            .child(options)
            .into_any_element();
        let mut footer = Vec::new();
        if index > 0 {
            footer.push(
                self.question_button(
                    "question-back",
                    "←",
                    json!({"type":"questionBack"}),
                    busy,
                    false,
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        if question["allowCustom"] != false {
            let key = format!("{}:{index}", prompt);
            if self
                .answer_input
                .as_ref()
                .is_none_or(|(previous, _)| previous != &key)
            {
                let input = cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .submit_on_enter(true)
                        .auto_grow(1, 4)
                        .placeholder("Or write your own answer…")
                        .default_value(text(&draft, "other"))
                });
                self.answer_subscription = Some(cx.subscribe_in(&input,window,|this,input,event:&InputEvent,window,cx| match event {
                    InputEvent::Change => this.invoke(json!({"type":"questionText","text":input.read(cx).value().to_string()}),cx),
                    InputEvent::PressEnter { shift:false,.. } => this.invoke(json!({"type":"questionNext"}),cx),
                    InputEvent::Focus => super::focus::reclaim_keyboard_focus(window),
                    _=>{},
                }));
                self.answer_input = Some((key, input));
            }
            footer.push(
                div()
                    .flex_1()
                    .min_w(px(160.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .px(px(10.0 * s))
                    .py(px(4.0 * s))
                    .rounded(px(9.0 * s))
                    .border_1()
                    .border_color(p.input_border)
                    .bg(if p.light { p.background } else { p.card_row })
                    .child(
                        gpui::svg()
                            .path("titlebar/pencil.svg")
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        Textarea::new(&self.answer_input.as_ref().unwrap().1)
                            .aria_label("Your answer")
                            .disabled(busy)
                            .placeholder_color(p.muted.opacity(0.75))
                            .caret_color(p.foreground)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .p_0()
                            .line_height(px(22.0 * s))
                            .text_color(p.foreground)
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.5 * s)),
                    )
                    .into_any_element(),
            );
        } else {
            footer.push(div().min_w_0().flex_1().into_any_element());
        }
        // CDXC:SessionChat 2026-09-16 DECISION: User: the question's X was the interrupt, an action rather than a dismiss, and sat beside the collapse chevron; it is a Cancel button in the footer instead. The approval keeps its circled X since it has no chevron (approval.rs).
        footer.push(
            self.question_button(
                "question-cancel",
                "Cancel",
                json!({"type":"questionCancel"}),
                busy,
                false,
                p,
                cx,
            )
            .into_any_element(),
        );
        footer.push(self.ask_primary_button(
            "question-next",
            text(&self.snapshot["questionCard"]["controls"], "label"),
            Some("↵".to_string()),
            None,
            json!({"type":"questionNext"}),
            busy || self.snapshot["questionCard"]["controls"]["disabled"] == true,
            p,
            cx,
        ));
        // CDXC:SessionChat 2026-10-11 DECISION: User picked "Clear ask, clear answer": the header names the ask ("Needs your answer") over the question's own header and its place ("Auth method · 1 of 2"), and short progress bars sit by the chevron while the card is open (they replace the 2026-09-21 "question N of M" counter there). Folded, the detail line shows the question instead.
        let detail = if collapsed {
            text(question, "question")
        } else if count > 1 {
            if header.is_empty() {
                format!("{} of {count}", index + 1)
            } else {
                format!("{header} · {} of {count}", index + 1)
            }
        } else {
            header
        };
        let mut trailing = Vec::new();
        if count > 1 && !collapsed {
            trailing.push(progress_dots(index, count, p));
        }
        trailing.push(chevron(!collapsed, p));
        let shows_body = !collapsed || motion.is_some();
        let toggle_key = collapse_key.clone();
        let head = press_head(
            ask_head(
                "titlebar/help-circle.svg",
                AskTone::Ask,
                "Needs your answer".to_string(),
                detail,
                trailing,
                p,
            ),
            "question-header",
            if collapsed {
                "Show the question and its options"
            } else {
                "Hide the question and its options"
            },
            true,
            p,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if !this.collapsed.remove(&toggle_key) {
                this.collapsed.insert(toggle_key.clone());
            }
            cx.notify();
        }))
        .into_any_element();
        Some(self.ask_card(
            head,
            shows_body.then_some(body),
            footer,
            Some(AskBodyMotion {
                key: &collapse_key,
                frame: motion,
            }),
            p,
        ))
    }

    /// CDXC:SessionChat 2026-10-14 DECISION:
    /// User: "please make the buttons look the same" (2026-10-09, no borderless text buttons), then "always use filled buttons everywhere" (CDXC:AppModal 2026-10-14 on `neutral_button_fills`). Every question, async-question and approval footer button (Back, Cancel, Skip included) takes the chat's neutral fill instead of the 2026-10-09 control border; the ghost variant stays gone here and in the phone's `CardButton` (apps/mobile/app/src/chat/native/cards/primitives.tsx).
    pub(super) fn question_button(
        &self,
        id: &'static str,
        label: &str,
        action: serde_json::Value,
        disabled: bool,
        wide: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.to_owned())
            .h(px(28.0 * s))
            .px(px(12.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0 * s))
            .border_1()
            .chat_filled(p.neutral_button_fills(), !disabled)
            .text_color(p.foreground)
            .font_weight(gpui::FontWeight::NORMAL)
            .when(wide, |button| button.min_w(px(96.0 * s)))
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| button.chat_cursor_pointer())
            .child(label.to_owned())
            .when(!disabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| this.invoke(action.clone(), cx)))
            })
    }
}
