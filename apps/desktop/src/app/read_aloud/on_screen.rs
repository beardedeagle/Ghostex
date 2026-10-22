//! "The chat on screen" for Read aloud (`setOnScreen`): the chat auto-read listens to, and the
//! one whose speech stops when it leaves the screen.

use crate::*;

impl GhostexGpuiApp {
    /// Tells the chat host which chat is on screen, from this window's render: the focused
    /// session's chat while the window is the active one and shows that chat (not its terminal).
    ///
    /// CDXC:VoiceMode 2026-10-22 WHY: the window the reader used last decides, so with several windows exactly one chat is on screen (the spec's "only one speaks"), and with Ghostex in the background the last focused window's chat stays on screen. A window that is not active reports nothing; GPUI draws a window again whenever it gains or loses focus, so the switch is heard at once. Only the active pane's session counts, so a chat in another split pane is not on screen.
    pub(crate) fn sync_read_aloud_on_screen(&self, window: &Window, cx: &gpui::App) {
        if !window.is_window_active() {
            return;
        }
        let key = self
            .focused_agents_or_companion_shell_session_id()
            .filter(|session| self.agents_chat_mode_sessions.contains(session))
            .and_then(|session| self.native_chat_views.get(&session))
            .and_then(|view| {
                view.read(cx)
                    .runtime
                    .as_ref()
                    .map(|runtime| runtime.key().to_string())
            });
        crate::app::gx_chat::read_aloud_on_screen(key);
    }
}
