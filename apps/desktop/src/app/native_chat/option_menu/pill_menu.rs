//! A working-row pill's floating menu (`chat_pills.rs`): the rows the Threads, Tasks or Subagents
//! panel used to show above the composer, drawn in a chat menu window over the transcript.

use super::window::ChatOptionMenuPanel;
use crate::app::native_chat::state::NativeChatView;
use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement as _, Styled as _, div, px};
use serde_json::{Value, json};
use std::sync::Arc;

impl ChatOptionMenuPanel {
    /// The chat draws the body with its own listeners, so a row's click reaches the chat as it did
    /// when the panel lived in the chat's window.
    pub(super) fn render_pill_menu(&self, pill: &Value, cx: &mut Context<Self>) -> AnyElement {
        let id = pill["id"].as_str().unwrap_or_default().to_owned();
        let height = pill["height"].as_f64().unwrap_or(0.0) as f32;
        let (appearance, chat) = {
            let menu = self.menu.read(cx);
            (menu.appearance.clone(), menu.chat.clone())
        };
        let body = chat
            .update(cx, |chat, cx| chat.pill_menu_body(&id, &appearance, cx))
            .ok()
            .flatten();
        div()
            .flex_shrink_0()
            .h(px(height * appearance.scale))
            .min_h_0()
            .flex()
            .flex_col()
            .children(body)
            .into_any_element()
    }

    /// The chat changed under an open pill menu: redraw it, follow its rows' height, and close it once
    /// its panel is gone (the subagents finished) or a row opened the subagent viewer over the pane.
    pub(super) fn pill_menu_changed(
        &mut self,
        chat: &Entity<NativeChatView>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self
            .rows
            .first()
            .and_then(|row| row["pillMenu"]["id"].as_str())
            .map(str::to_owned)
        else {
            return;
        };
        let snapshot = &chat.read(cx).snapshot;
        let height = crate::app::native_chat::chat_pills::pill_menu_height(&id, snapshot);
        let Some(height) = height.filter(|_| !snapshot["subagent"].is_object()) else {
            self.menu.update(cx, |menu, cx| menu.close(None, cx));
            return;
        };
        cx.notify();
        if self.heights.first() == Some(&height) {
            return;
        }
        let rows = vec![json!({"pillMenu": {"id": id, "height": height}})];
        self.heights = vec![height];
        self.rows = Arc::new(rows.clone());
        let depth = self.depth;
        self.menu.update(cx, |menu, cx| {
            let outer = px((menu.metrics().chrome() + height) * menu.appearance.scale);
            menu.refit_panel_height(depth, outer, rows, cx);
        });
    }
}
