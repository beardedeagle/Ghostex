//! The body of a tool permission card: the file it touches, the command, diff or input, and why
//! the agent's CLI asks. Drawn from the chat core's `approval` on `terminalNotice`
//! (`packages/gx-chat-core/src/questions/permission_card.rs`, which holds the DECISION); the
//! phone's `PermissionDetails.tsx` draws the same parts.

use super::tone_card::tone_look;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

impl NativeChatView {
    /// The card's details, above its choices. A folded card cuts the code to the core's preview;
    /// "N more lines" opens the card like its header does.
    pub(super) fn permission_details(
        &self,
        approval: &Value,
        collapsed: bool,
        card_key: &str,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        let mut parts = Vec::new();
        let mono = |content: String| {
            div()
                .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                .text_size(px(12.0 * s))
                .line_height(px(17.0 * s))
                .child(content)
        };
        let path = text(approval, "path");
        if !path.is_empty() {
            parts.push(
                mono(path)
                    .id("permission-path")
                    .text_color(p.card_muted)
                    .into_any_element(),
            );
        }
        let code = text(approval, "code");
        if !code.is_empty() {
            let preview = text(approval, "codePreview");
            let folded = collapsed && !preview.is_empty();
            let mut well = div()
                .id("permission-code")
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .rounded(px(8.0 * s))
                .px(px(12.0 * s))
                .py(px(10.0 * s))
                .bg(p.border.opacity(0.3))
                .text_color(p.foreground)
                .child(mono(if folded { preview } else { code }));
            if !folded {
                well = well.max_h(px(240.0 * s)).overflow_y_scroll();
            }
            if folded {
                let key = card_key.to_string();
                well = well.child(
                    div()
                        .id("permission-code-more")
                        .role(gpui::Role::Button)
                        .aria_label("Show the whole command")
                        .text_size(px(12.0 * s))
                        .line_height(px(17.0 * s))
                        .text_color(p.muted)
                        .chat_cursor_pointer()
                        .hover(|style| style.text_color(p.foreground))
                        .child(format!("+ {}", text(approval, "codeMore")))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.expanded.insert(key.clone());
                            cx.notify();
                        })),
                );
            } else if !text(approval, "codeCut").is_empty() {
                well = well.child(
                    div()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(text(approval, "codeCut")),
                );
            }
            parts.push(well.into_any_element());
        }
        let reason = &approval["reason"];
        if reason.is_object() {
            let warning = reason["tone"] == "warning";
            let look = tone_look(if warning { "warning" } else { "info" }, p);
            let hue = look.hue.unwrap_or(p.muted);
            let label = text(reason, "label");
            parts.push(
                div()
                    .id("permission-reason")
                    .flex()
                    .items_start()
                    .gap(px(8.0 * s))
                    .px(px(10.0 * s))
                    .py(px(8.0 * s))
                    .rounded(px(8.0 * s))
                    .border_1()
                    .when(warning, |row| {
                        row.border_color(hue.opacity(0.28)).bg(hue.opacity(0.08))
                    })
                    .when(!warning, |row| {
                        row.border_color(p.control_border.opacity(0.65))
                    })
                    .child(
                        gpui::svg()
                            .path(look.icon)
                            .mt(px(2.0 * s))
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .text_color(if warning { hue } else { p.muted }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.0 * s))
                            .line_height(px(18.0 * s))
                            .text_color(if warning { p.foreground } else { p.card_muted })
                            .when(!label.is_empty(), |column| {
                                column.child(
                                    div()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(p.foreground)
                                        .child(label),
                                )
                            })
                            .child(text(reason, "text")),
                    )
                    .into_any_element(),
            );
        }
        parts
    }
}
