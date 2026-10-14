//! The "Previous question" pill beside Scroll to bottom, the jump to a question it (or a Q-tag link)
//! asks for, and the brief highlight on the form or page card it lands on. Which questions are
//! unanswered, which one a click goes to and the row it sits in are the chat core's
//! (`openQuestions`, packages/gx-chat-core/src/questions/open_questions.rs); this scrolls and draws.
//!
//! CDXC:SessionChat 2026-10-14 SEE-ALSO:
//! The phone draws the same pill and highlight in apps/mobile/app/src/chat/native/transcript/Transcript.tsx and OpenQuestions.tsx; the rules are packages/gx-chat-core/src/transcript/open_questions.rs. Change them together.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::json;
use std::time::Duration;
use web_time::Instant;

/// The question a jump landed on, highlighted for a moment, and the row the transcript still has
/// to settle once it has measured it (`settle_question_row`).
pub(super) struct QuestionFlash {
    target: String,
    started: Instant,
    settle_row: Option<usize>,
}

/// Room kept below a tall reply's end once it is brought into view.
const END_MARGIN: f32 = 16.0;

impl NativeChatView {
    /// The pill's text while the chat has unanswered questions.
    pub(super) fn open_question_pill_text(&self) -> Option<String> {
        let open = &self.snapshot["openQuestions"];
        (open["count"].as_u64().unwrap_or(0) > 0)
            .then(|| open["text"].as_str().map(str::to_string))
            .flatten()
    }

    /// The pill's click: the core picks the question.
    pub(super) fn previous_question(&mut self, cx: &mut Context<Self>) {
        self.invoke(json!({"type": "previousQuestion"}), cx);
    }

    /// Scrolls to the reply holding the question once per jump (the core bumps `revision`), then
    /// highlights the question for a moment.
    pub(super) fn sync_open_question_jump(&mut self, window: &mut Window) {
        let jump = &self.snapshot["openQuestions"]["jump"];
        let revision = jump["revision"].as_u64();
        match (self.open_question_jump_revision, revision) {
            // The first document this pane reads: a jump made before it opened is not replayed.
            (None, _) => self.open_question_jump_revision = Some(revision.unwrap_or(0)),
            (Some(seen), Some(revision)) if revision != seen => {
                self.open_question_jump_revision = Some(revision);
                if let Some(item) = jump["item"].as_u64() {
                    // `scroll_to` (not `scroll_to_reveal_item`) also stops a chat at the bottom
                    // from following its tail straight back (search.rs, `sync_search_scroll`).
                    self.list.scroll_to(gpui::ListOffset {
                        item_ix: item as usize,
                        offset_in_item: px(0.0),
                    });
                    self.question_flash = Some(QuestionFlash {
                        target: jump["target"].as_str().unwrap_or_default().to_string(),
                        started: Instant::now(),
                        settle_row: Some(item as usize),
                    });
                    return window.request_animation_frame();
                }
            }
            _ => {}
        }
        self.settle_question_row();
        if let Some(flash) = &self.question_flash {
            if flash.started.elapsed() >= flash_duration() {
                self.question_flash = None;
            } else {
                window.request_animation_frame();
            }
        }
    }

    /// Once the row a jump scrolled to has been laid out: a reply taller than the transcript is
    /// moved up until its end sits at the bottom, since a form or page card usually closes the
    /// reply (the phone aligns the row's end the same way).
    ///
    /// CDXC:SessionChat 2026-10-14 WHY:
    /// Bringing the card itself into view from its own measured bounds was tried and dropped: inside the transcript's list a row's descendants report bounds shifted by the rows above it, so the nudge overshot by a whole row. The row's own bounds (`bounds_for_item`) are right, so the alignment uses only those.
    fn settle_question_row(&mut self) {
        let Some(item) = self
            .question_flash
            .as_ref()
            .and_then(|flash| flash.settle_row)
        else {
            return;
        };
        let Some(row) = self.list.bounds_for_item(item) else {
            return;
        };
        if let Some(flash) = self.question_flash.as_mut() {
            flash.settle_row = None;
        }
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let viewport = self.list.viewport_bounds().size.height;
        let overflow = row.size.height - viewport + px(END_MARGIN * scale);
        if overflow > px(0.0) {
            self.list.scroll_to(gpui::ListOffset {
                item_ix: item,
                offset_in_item: overflow,
            });
        }
    }

    /// A question form or page card, with the jump's highlight around it while it lasts.
    pub(super) fn open_question_frame(
        &self,
        target: Option<&str>,
        card: AnyElement,
        p: &ChatAppearance,
    ) -> AnyElement {
        let Some(flash) = self
            .question_flash
            .as_ref()
            .filter(|flash| target == Some(flash.target.as_str()))
        else {
            return card;
        };
        let s = p.scale;
        let (_, border, _) = question_tone(p.light);
        // Full strength, then fading out over the last third.
        let progress =
            flash.started.elapsed().as_secs_f32() / flash_duration().as_secs_f32().max(0.001);
        let strength = ((1.0 - progress) * 3.0).clamp(0.0, 1.0);
        div()
            .relative()
            .w_full()
            .child(card)
            // Painted, not hit-tested: a plain div with no id takes no input.
            .child(
                div()
                    .absolute()
                    .top(px(-4.0 * s))
                    .left(px(-4.0 * s))
                    .right(px(-4.0 * s))
                    .bottom(px(-4.0 * s))
                    .rounded(px(18.0 * s))
                    .border_2()
                    .border_color(border.opacity(strength))
                    .bg(border.opacity(0.08 * strength)),
            )
            .into_any_element()
    }

    /// The pill in the chat's own window (no window glass).
    pub(super) fn previous_question_pill(
        &self,
        text: String,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (fill, border, color) = question_tone(p.light);
        let label = self.snapshot["openQuestions"]["accessibilityLabel"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        super::scroll_bottom::pill_shape(div(), p.scale)
            .id("chat-previous-question")
            .tab_index(0)
            .role(gpui::Role::Button)
            .aria_label(label)
            .chat_cursor_pointer()
            .border_color(border)
            .bg(fill)
            .text_color(color)
            .focus_visible(|style| style.border_color(p.ring))
            .child(text)
            .on_click(cx.listener(|chat, _, _, cx| chat.previous_question(cx)))
            .into_any_element()
    }
}

fn flash_duration() -> Duration {
    Duration::from_millis(super::scroll_bottom::question_flash_ms())
}

/// The pill's fill, border and text (`questionTone` in `scroll-bottom.json`).
pub(super) fn question_tone(light: bool) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
    super::scroll_bottom::question_tone(light)
}

impl NativeChatView {
    /// The pill when the chat window is glass: its own blurred window
    /// (frosted_overlay_window.rs), placed on an invisible pill of the same size laid out here.
    pub(super) fn previous_question_placeholder(
        &self,
        text: Option<String>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let overlay = super::frosted_overlay_window::FrostedOverlay::PreviousQuestion;
        let shown = text.is_some() && !self.frosted_overlay_covered(overlay, cx);
        let report = self.frosted_overlay_reporter(overlay, shown, cx);
        match text.filter(|_| shown) {
            Some(text) => super::scroll_bottom::placeholder_pill(text, p)
                .child(report)
                .into_any_element(),
            None => div().absolute().size_0().child(report).into_any_element(),
        }
    }
}
