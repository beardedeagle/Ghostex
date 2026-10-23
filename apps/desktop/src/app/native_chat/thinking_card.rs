//! The running turn's thinking drawn as a one-line card from its first word, instead of the open
//! thinking lane (`thinking.rs`). gx-chat-core marks the rows (`thinkingCard`,
//! `transcript/thinking_card.rs`, where the user's decision is); this draws them with the fold's
//! card look (`SINGLE_LINE_CARD`): a brain tile, "Thinking…" while the thought is being written or
//! "Thought" once the agent moved on, and the thought's newest finished line while it is closed.
//! One click opens or closes the thought below the header. The phone's `ThinkingCard` in
//! `MessageRow.tsx` is the same card.

use super::cards::{SINGLE_LINE_CARD, card_hover_fill};
use super::transcript::text;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

/// React's secondary-block cap (`thinking.rs`): an opened thought scrolls past this.
const OPEN_CAP: f32 = 288.0;

impl NativeChatView {
    pub(super) fn thinking_card(
        &self,
        id: &str,
        message: &Value,
        body: String,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let card = &*SINGLE_LINE_CARD;
        let info = &message["thinkingCard"];
        let live = info["live"] == true;
        let key = format!("thinking-card:{id}");
        let expanded = self.is_expanded(&key, false);
        let preview = text(info, "preview");
        let lane = p.thinking_lane();
        let title = if live { "Thinking…" } else { "Thought" };

        let header = div()
            .id(SharedString::from(format!("{key}:header")))
            .role(gpui::Role::Button)
            .aria_label(if preview.is_empty() {
                title.to_string()
            } else {
                format!("{title} {preview}")
            })
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .w_full()
            .h(px(card.header_height(s)))
            .flex_shrink_0()
            .gap(px(10.0 * s))
            .px(px(card.padding_x * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(card_hover_fill(p)))
            .child(card.tile("titlebar/brain.svg", p))
            .child(
                card.title(div().flex_shrink_0(), p)
                    .text_color(p.foreground)
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0 * s))
                    .text_color(p.muted)
                    .truncate()
                    .child(if expanded { String::new() } else { preview }),
            )
            .on_click({
                let key = key.clone();
                cx.listener(move |this, _, _, cx| {
                    if expanded {
                        this.expanded.remove(&key);
                        this.collapsed.insert(key.clone());
                    } else {
                        this.collapsed.remove(&key);
                        this.expanded.insert(key.clone());
                    }
                    cx.notify();
                })
            });

        let mut shell = card
            .shell(div(), p)
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header);
        if expanded {
            let body_key = SharedString::from(format!("{key}:body"));
            // While it is being written the open card keeps the newest lines in view, the way a
            // terminal does, until the reader scrolls up; scrolling back to the end follows again.
            if live {
                let handle = self
                    .nested_scrolls
                    .borrow_mut()
                    .entry(body_key.clone())
                    .or_default()
                    .clone();
                if handle.offset().y <= -handle.max_offset().y + px(2.0) {
                    handle.scroll_to_bottom();
                }
            }
            shell = shell.child(
                self.nested_scroll(
                    body_key,
                    div()
                        .min_w_0()
                        .max_h(px(OPEN_CAP * s))
                        .px(px(card.padding_x * s))
                        .pb(px(12.0 * s))
                        .child(self.markdown(
                            format!("thinking-card-body:{id}"),
                            body,
                            &message["markdownReferences"],
                            &lane,
                            cx,
                        )),
                ),
            );
        }
        div()
            .w_full()
            .pb(px(13.0 * s))
            .child(shell)
            .into_any_element()
    }
}
