//! Side chat: messages sent with `/btw` at their start, a side question for Claude and a side
//! conversation for Codex.
//!
//! CDXC:SessionChat 2026-09-30 DECISION:
//! User: a "Side chat" button in the chat composer's More actions for Claude puts a "Side Chat" pill in the chat box; what is sent while it is there reaches the agent CLI with `/btw` at its start, and the user can delete the pill. The pill is the `/btw ` prefix itself, drawn as a reference pill, so drafts, other devices and the send path need nothing new. Sending a side chat switches Side chat off: the pill leaves with the message (supersedes the 2026-09-27 choice to keep it after a send).
//! SEE-ALSO: apps/desktop/src/app/native_chat/actions.rs, apps/mobile/app/src/chat/native/composer/menus.ts, server/src/session_chat_claude_panel.rs, server/src/session_chat_codex_side.rs.

use crate::menus::option_menus::DraftAgent;
use crate::state::ChatState;

/// What Side chat puts at the start of the chat box, and what the agent reads as a side question.
pub const SIDE_CHAT_PREFIX: &str = "/btw ";

/// The pill's words in the chat box.
pub const SIDE_CHAT_LABEL: &str = "Side Chat";

/// The CLI behind this session: a custom agent reports its base CLI, the way the account panel
/// resolves it (menus/controls.rs).
pub(crate) fn agent_family(state: &ChatState) -> Option<String> {
    DraftAgent::list(state.session.available_agents.as_ref())
        .and_then(|agents| {
            agents
                .iter()
                .find(|row| {
                    Some(row.agent_id.as_str()) == state.session.session_agent_id.as_deref()
                })
                .and_then(|row| row.base_agent_id.clone())
        })
        .or_else(|| state.session.agent.clone())
}

/// Whether this session's agent takes `/btw`: Claude Code answers it as a side question, and
/// Codex opens a side conversation, directly or as the base of a custom agent.
pub fn side_chat_available(state: &ChatState) -> bool {
    matches!(
        agent_family(state).as_deref(),
        Some("claude" | "openclaude" | "codex")
    )
}

/// Whether a sent message leaves Chat for the terminal: a Codex side chat, whose side conversation
/// the chat cannot show.
///
/// CDXC:SessionChat 2026-09-30 DECISION: User: in Codex chats, sending a Side chat message switches to the terminal and the user deals with the side conversation there. gxserver leaves the side conversation before the next chat message and when the chat view comes back (server/src/session_chat_codex_side.rs).
pub fn sent_side_chat_opens_terminal(state: &ChatState, text: &str) -> bool {
    text.starts_with(SIDE_CHAT_PREFIX)
        && !text[SIDE_CHAT_PREFIX.len()..].trim().is_empty()
        && agent_family(state).as_deref() == Some("codex")
}

/// The Side chat prefix, when Side chat is available.
pub fn side_chat_prefix(state: &ChatState) -> Option<String> {
    side_chat_available(state).then(|| SIDE_CHAT_PREFIX.to_string())
}

/// The chat box with Side chat switched on (prefix added) or off (prefix removed).
pub fn toggle_side_chat(text: &str) -> String {
    match text.strip_prefix(SIDE_CHAT_PREFIX) {
        Some(rest) => rest.to_string(),
        None => format!("{SIDE_CHAT_PREFIX}{}", text.trim_start()),
    }
}
