//! Focus Chat Box, Copy Last Code Block, Copy Last Reply, Toggle Summary Mode, Maximize Chat Box and
//! Files and Links, answered for the chat of the focused session. The per-chat work is in `native_chat/chat_hotkeys.rs`.
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Hotkeys 2026-09-25 WHY:
    /// The chat's key context only covers its chat box, so binding these there made Shift+Esc fire only when the chat box already had focus (the user found it did nothing). They are app hotkeys instead, acting on the chat shown for the focused session from anywhere in the window. Returns false when that session is not in chat view, so the key keeps its ordinary meaning.
    pub(crate) fn run_focused_chat_hotkey(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return false;
        }
        let Some(view) = self.native_chat_views.get(&session_id).cloned() else {
            return false;
        };
        match action_id {
            "focusChatComposer" => {
                self.reclaim_gpui_root_for_native_chat_composer(window);
                view.update(cx, |view, cx| view.focus_composer_from_hotkey(window, cx));
                true
            }
            "copyLastChatCodeBlock" => {
                view.update(cx, |view, cx| view.copy_last_reply_from_hotkey(true, cx))
            }
            "copyLastChatReply" => {
                view.update(cx, |view, cx| view.copy_last_reply_from_hotkey(false, cx))
            }
            "toggleChatSummaryMode" => {
                view.update(cx, |view, cx| {
                    view.invoke(serde_json::json!({"type":"toggleSummary"}), cx)
                });
                true
            }
            "toggleChatComposerMaximized" => {
                view.update(cx, |view, cx| view.toggle_maximized(window, cx));
                true
            }
            "toggleChatFilesLinks" => {
                view.update(cx, |view, cx| {
                    view.invoke(serde_json::json!({"type":"filesLinks"}), cx)
                });
                true
            }
            _ => false,
        }
    }

    /// Cmd+F that reached the window root while the focused session shows its chat: the chat's own
    /// search opens, never a search in the terminal parked behind it. Returns false when that
    /// session is not in chat view.
    ///
    /// CDXC:SessionChat 2026-10-01 WHY:
    /// The chat claims Cmd+F with its own binding, and the shell's terminal Find is bound at the root with no context, which GPUI ranks as deep as the focused field's own context, so the two are separated only by the order they were registered in. Whenever the root's Find got the keystroke for a chat pane it searched the terminal parked behind the chat, and the user saw Cmd+F do nothing.
    pub(crate) fn open_search_in_focused_chat(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return false;
        }
        let Some(view) = self.native_chat_views.get(&session_id).cloned() else {
            return false;
        };
        self.reclaim_gpui_root_for_native_chat_composer(window);
        view.update(cx, |view, cx| view.open_search(window, cx));
        true
    }
}
