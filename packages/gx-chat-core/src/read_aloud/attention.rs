//! The line auto-read says when the agent stops to wait for you.

use crate::questions::gates::{notice_visible, prompt_key, question_card_visible};
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_state::notice_dismiss_key;
use crate::read_aloud::speakable::{APPROVAL_LINE, ASKING_LINE};
use crate::state::{ChatState, Notice};

/// The question or approval the chat is waiting on right now, keyed so one prompt is announced
/// once, or `None` when nothing waits on you.
///
/// CDXC:VoiceMode 2026-10-21 DECISION:
/// User (Q34): when a turn ends with a question card, auto-read says "The agent is asking you a question". A question card is not an agent message, so it is announced by this line rather than read.
///
/// CDXC:VoiceMode 2026-10-21 WHY:
/// A turn that ends waiting on a permission prompt (the approval card, or the agent's own permission dialog on its screen) is announced the same way, "The agent is waiting for your approval": while you drive, a silent approval stalls the agent as surely as a silent question. This one is the orchestrator's judgment, not the user's answer.
pub fn current(state: &ChatState) -> Option<Notice> {
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    if let Some(prompt) = prompt.as_ref() {
        if question_card_visible(state, Some(prompt), notice.as_ref()) {
            let key = format!(
                "{}|{}",
                prompt_key(Some(prompt)).unwrap_or_default(),
                prompt.tool_use_id().unwrap_or_default()
            );
            return Some(Notice {
                key,
                line: if prompt.is_question() {
                    ASKING_LINE
                } else {
                    APPROVAL_LINE
                },
            });
        }
    }
    let notice = notice?;
    (notice.kind == "permissionPrompt" && notice_visible(state)).then(|| Notice {
        key: format!(
            "notice|{}",
            notice_dismiss_key(Some(&notice)).unwrap_or_default()
        ),
        line: APPROVAL_LINE,
    })
}
