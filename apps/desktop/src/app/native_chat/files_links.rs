//! Files and links: the card above the composer that lists every attachment, file and link the
//! session mentioned (a coordinator's threads included), opened from its toolbar button. The core
//! owns what is listed, the sections, the filter and where a jump lands
//! (`packages/gx-chat-core/src/composer/files_links.rs`); this draws it, opens a row's item and
//! scrolls the transcript once per jump.

use super::side_question::{answering_placeholder, side_button};
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::{Value, json};

fn row_icon(icon: &str) -> &'static str {
    match icon {
        "photo" => "titlebar/photo.svg",
        "folder" => "titlebar/folder.svg",
        "link" => "titlebar/link.svg",
        _ => "titlebar/file.svg",
    }
}

impl NativeChatView {
    /// Scrolls the transcript to the row a jump chose, once per click (the core bumps
    /// `revision`).
    pub(super) fn sync_files_links_jump(&mut self) {
        let jump = &self.snapshot["filesLinks"]["jump"];
        let Some(revision) = jump["revision"].as_u64() else {
            self.files_links_jump_revision = 0;
            return;
        };
        if revision == self.files_links_jump_revision {
            return;
        }
        self.files_links_jump_revision = revision;
        let Some(item) = jump["item"].as_u64() else {
            return;
        };
        // `scroll_to` (not `scroll_to_reveal_item`) also stops a chat at the bottom from following
        // its tail straight back (search.rs, `sync_search_scroll`).
        self.list.scroll_to(gpui::ListOffset {
            item_ix: item as usize,
            offset_in_item: px(0.0),
        });
    }

    /// The filter field exists while the open card is long enough to offer it.
    fn sync_files_links_input(
        &mut self,
        wanted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !wanted {
            self.files_links_input = None;
            self.files_links_subscription = None;
            return;
        }
        if self.files_links_input.is_some() {
            return;
        }
        let query = text(&self.snapshot["filesLinks"], "query");
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("Filter by name, path or link");
            state.set_value(query, window, cx);
            state
        });
        self.files_links_subscription =
            Some(
                cx.subscribe_in(&input, window, |chat, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let query = input.read(cx).value().to_string();
                        chat.invoke(json!({"type":"filesLinksFilter","text":query}), cx);
                    }
                }),
            );
        self.files_links_input = Some(input);
    }

    /// One row: its icon, name, tag and where it lives, with the jumps on the right. A click opens
    /// the item: a picture in the image viewer (with the group's other pictures beside it), anything
    /// else the way the transcript's own pill or link opens it.
    fn files_links_row(
        &self,
        row: &Value,
        gallery: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let label = text(row, "label");
        let detail = text(row, "detail");
        let meta = text(row, "meta");
        let open = row["open"].clone();
        let image = row["image"].clone();
        let gallery: Vec<Value> = gallery.to_vec();
        let index = gallery
            .iter()
            .position(|other| *other == image)
            .unwrap_or(0);
        let jumps: Vec<AnyElement> = row["jumps"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(position, jump)| {
                let label = text(jump, "label");
                let tooltip = text(jump, "tooltip");
                let command = jump["command"].clone();
                div()
                    .id(SharedString::from(format!(
                        "files-links-jump:{key}:{position}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{label}: {tooltip}"))
                    .flex_shrink_0()
                    .px(px(7.0 * s))
                    .py(px(2.0 * s))
                    .rounded(px(5.0 * s))
                    .border_1()
                    .chat_filled(p.neutral_button_fills(), true)
                    .text_size(px(11.0 * s))
                    .text_color(p.primary)
                    .chat_cursor_pointer()
                    .child(label)
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
                    .into_any_element()
            })
            .collect();
        div()
            .id(SharedString::from(format!("files-links-row:{key}")))
            .role(gpui::Role::Button)
            .aria_label(format!("Open {label}"))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(6.0 * s))
            .py(px(4.0 * s))
            .rounded(px(6.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.6)))
            .child(
                svg()
                    .path(row_icon(&text(row, "icon")))
                    .flex_shrink_0()
                    .size(px(15.0 * s))
                    .text_color(p.muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.0 * s))
                                    .text_color(p.foreground)
                                    .child(label.clone()),
                            )
                            .when_some(row["tag"].as_str().map(str::to_string), |this, tag| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .px(px(5.0 * s))
                                        .rounded(px(4.0 * s))
                                        .bg(p.border)
                                        .text_size(px(10.0 * s))
                                        .text_color(p.muted)
                                        .child(tag),
                                )
                            }),
                    )
                    // Where it lives and when it was mentioned share one line, so a long list
                    // shows more rows; the path gives way first.
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(8.0 * s))
                            .text_size(px(11.5 * s))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(p.muted)
                                    .child(detail.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0 * s))
                                    .text_color(p.muted.opacity(0.75))
                                    .child(meta),
                            ),
                    ),
            )
            .children(jumps)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(detail.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if image.is_object() && image["transport"] != "none" {
                    this.open_image_viewer(gallery.clone(), index, cx);
                } else {
                    this.invoke(open.clone(), cx);
                }
            }))
            .into_any_element()
    }

    /// The card above the composer: loading, the list in its three sections, or why there is
    /// none, with Refresh and Close.
    pub(super) fn render_files_links(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let files_links = self.snapshot["filesLinks"].clone();
        let open = files_links["open"] == true;
        self.sync_files_links_input(open && files_links["filter"] == true, window, cx);
        if !open {
            return None;
        }
        let s = p.scale;
        let loading = files_links["loading"] == true;
        let caption = text(&files_links, "caption");
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .child(
                svg()
                    .path("titlebar/link.svg")
                    .size(px(14.0 * s))
                    .text_color(p.primary),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(p.foreground)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Files and links"),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .child(if loading && caption.is_empty() {
                        "Reading the session…".to_string()
                    } else {
                        caption
                    }),
            )
            .into_any_element();
        let mut body = Vec::new();
        if let Some(input) = self.files_links_input.clone() {
            body.push(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(p.input_border)
                    .bg(p.composer_background)
                    .px(px(10.0 * s))
                    .py(px(4.0 * s))
                    .child(
                        svg()
                            .path("titlebar/search.svg")
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&input)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .w_full()
                                .p_0()
                                .text_size(px(13.0 * s))
                                .text_color(p.primary)
                                .placeholder_color(p.muted.opacity(0.6))
                                .caret_color(p.foreground),
                        ),
                    )
                    .into_any_element(),
            );
        }
        let sections = files_links["sections"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if loading && sections.is_empty() {
            body.push(answering_placeholder(p));
        } else if let Some(error) = files_links["error"].as_str() {
            body.push(
                div()
                    .text_size(px(13.0 * s))
                    .text_color(gpui::rgb(0xef9999))
                    .child(error.to_string())
                    .into_any_element(),
            );
        } else if sections.is_empty() {
            body.push(
                div()
                    .text_size(px(13.0 * s))
                    .text_color(p.muted)
                    .child(text(&files_links, "emptyLabel"))
                    .into_any_element(),
            );
        } else {
            let mut list = div()
                .id("files-links-list")
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(10.0 * s))
                .max_h(px(window.viewport_size().height.as_f32() * 0.5))
                .overflow_y_scroll();
            for section in &sections {
                let id = text(section, "id");
                let mut column = div().min_w_0().flex().flex_col().gap(px(2.0 * s)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .pb(px(2.0 * s))
                        .child(
                            div()
                                .text_size(px(12.0 * s))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.foreground)
                                .child(text(section, "label")),
                        )
                        .child(
                            div()
                                .text_size(px(12.0 * s))
                                .text_color(p.muted)
                                .child(section["count"].as_u64().unwrap_or(0).to_string()),
                        ),
                );
                for group in section["groups"].as_array().into_iter().flatten() {
                    let title = text(group, "title");
                    if !title.is_empty() {
                        column = column.child(
                            div()
                                .pt(px(4.0 * s))
                                .px(px(6.0 * s))
                                .truncate()
                                .text_size(px(11.0 * s))
                                .text_color(p.muted)
                                .child(title),
                        );
                    }
                    let rows = group["rows"].as_array().cloned().unwrap_or_default();
                    let gallery: Vec<Value> = rows
                        .iter()
                        .filter(|row| {
                            row["image"].is_object() && row["image"]["transport"] != "none"
                        })
                        .map(|row| row["image"].clone())
                        .collect();
                    for row in &rows {
                        column = column.child(self.files_links_row(row, &gallery, p, cx));
                    }
                }
                let more = text(section, "moreLabel");
                if !more.is_empty() {
                    column = column.child(
                        div()
                            .id(SharedString::from(format!("files-links-more:{id}")))
                            .role(gpui::Role::Button)
                            .px(px(6.0 * s))
                            .pt(px(2.0 * s))
                            .text_size(px(12.0 * s))
                            .text_color(p.card_muted)
                            .chat_cursor_pointer()
                            .hover(|style| style.text_color(p.foreground))
                            .child(more)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.invoke(json!({"type":"filesLinksShowAll","section":id}), cx)
                            })),
                    );
                }
                list = list.child(column);
            }
            body.push(list.into_any_element());
        }
        let actions = vec![
            div().flex_1().into_any_element(),
            side_button(
                "files-links-refresh",
                Some("titlebar/refresh.svg"),
                "Refresh",
                None,
                loading,
                Some("Read the session again for anything mentioned since"),
                p,
                cx.listener(|this, _, _, cx| this.invoke(json!({"type":"filesLinksRefresh"}), cx)),
            ),
            side_button(
                "files-links-close",
                Some("titlebar/x.svg"),
                "Close",
                None,
                false,
                None,
                p,
                cx.listener(|this, _, _, cx| this.invoke(json!({"type":"filesLinksClose"}), cx)),
            ),
        ];
        Some(self.status_card_with_header(header, body, actions, p))
    }
}
