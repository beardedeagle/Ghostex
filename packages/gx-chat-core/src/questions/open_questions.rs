//! The "Previous question" pill and the jumps to a question: which forms and question pages are
//! still waiting on the reader, which one each click of the pill goes to, and the transcript row a
//! Q-tag link lands on. What counts as asked, answered and obsolete is
//! `transcript/open_questions.rs`; the renderers scroll to `jump.item` once per `jump.revision` and
//! highlight `jump.target` briefly.
//!
//! CDXC:SessionChat 2026-10-14 DECISION:
//! User (Q81 = C): a "Previous question ↑" pill next to Scroll to bottom while the chat has unanswered questions; each click scrolls to the next older unanswered one, wrapping, and highlights it briefly. A form whose answers are queued or sending is no longer waiting on the reader. Then: "I don't like the emoji in the Previous question thing. Remove that icon and remove the number." and "Disable this feature by default unless the user has it enabled in Settings, since I want to test it more.": the pill reads "Previous question ↑" and shows only while Settings > Chat > Previous question button (`chatPreviousQuestion`) is on. Supersedes the same day's "❓ 2 · Previous question ↑" pill shown to everyone. Q-tag links stay on either way: they only appear where the agent names a question and do nothing until clicked.

use serde::{Deserialize, Serialize};

use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::open_questions::{tag_target, Asker, AskerKind};

/// The document's `openQuestions`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestions {
    /// How many forms and question pages are unanswered; the pill shows while this is above zero.
    /// Zero while the Previous question setting is off, so the pill never shows.
    pub count: u32,
    /// The pill's text: "Previous question ↑".
    pub text: String,
    /// What a screen reader says for it.
    pub accessibility_label: String,
    /// The last jump, or `null` before the first.
    pub jump: Option<OpenQuestionJump>,
}

/// One jump to a question: scroll to `item` once per `revision`, then highlight `target` (the
/// form's id or the page's id).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestionJump {
    pub revision: u64,
    pub item: usize,
    pub target: String,
}

/// What the pill remembers between clicks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenQuestionsState {
    /// The question the last jump went to, which the next click steps back from.
    pub last: Option<String>,
    pub jump: Option<OpenQuestionJump>,
}

/// The forms and question pages still waiting on the reader, oldest first.
pub fn unanswered(state: &ChatState) -> Vec<&Asker> {
    state
        .transcript_view
        .askers
        .iter()
        .filter(|asker| !asker.answered && !asker.obsolete)
        .filter(|asker| {
            asker.kind != AskerKind::Form
                || state
                    .questions
                    .forms
                    .get(&asker.id)
                    .is_none_or(|form| form.send.is_none() && form.sent.is_none())
        })
        .collect()
}

/// The document's `openQuestions`.
pub fn project(state: &ChatState) -> OpenQuestions {
    let count = if state.core.previous_question_enabled {
        unanswered(state).len() as u32
    } else {
        0
    };
    OpenQuestions {
        count,
        text: "Previous question \u{2191}".to_string(),
        accessibility_label: match count {
            1 => "1 unanswered question: go to it".to_string(),
            _ => format!("{count} unanswered questions: go to the previous one"),
        },
        jump: state.questions.open_questions.jump.clone(),
    }
}

/// `previousQuestion`: the pill's click. It goes to the unanswered question before the one it went
/// to last (wrapping to the newest after the oldest), or to the newest when it has not gone
/// anywhere yet or the last one is no longer waiting.
///
/// CDXC:SessionChat 2026-10-14 WHY:
/// Starting again from the newest whenever the reader sat at the bottom was tried and dropped: the newest question usually sits at the bottom, so the second click landed on it again instead of stepping back.
pub fn previous(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let open: Vec<Asker> = unanswered(state).into_iter().cloned().collect();
    if open.is_empty() {
        return Vec::new();
    }
    let last = state.questions.open_questions.last.as_deref();
    let at = open
        .iter()
        .position(|asker| Some(asker.id.as_str()) == last);
    jump_to(state, context, &open[step(open.len(), at)])
}

/// Which of `count` unanswered questions (oldest first) the pill goes to, given where the last
/// jump went (`at`).
fn step(count: usize, at: Option<usize>) -> usize {
    match at {
        Some(index) => index.checked_sub(1).unwrap_or(count - 1),
        None => count - 1,
    }
}

/// A Q-tag link's click (`#gx-question=Q80`, `transcript/open_questions.rs`): the latest form or
/// page that asked the tag, answered or not.
pub fn jump_to_tag(state: &mut ChatState, context: &ChatContext, tag: &str) -> Vec<Effect> {
    let Some(asker) = tag_target(&state.transcript_view.askers, tag).cloned() else {
        return vec![Effect::Toast {
            level: "info".to_string(),
            message: format!("{tag} isn't asked in the loaded part of this chat."),
        }];
    };
    jump_to(state, context, &asker)
}

fn jump_to(state: &mut ChatState, context: &ChatContext, asker: &Asker) -> Vec<Effect> {
    let rows = crate::transcript::rows(state, context);
    let Some(item) = crate::composer::files_links::row_of(&rows, &asker.message_id, None) else {
        return Vec::new();
    };
    let revision = state
        .questions
        .open_questions
        .jump
        .as_ref()
        .map_or(1, |jump| jump.revision + 1);
    state.questions.open_questions = OpenQuestionsState {
        last: Some(asker.id.clone()),
        jump: Some(OpenQuestionJump {
            revision,
            item,
            target: asker.id.clone(),
        }),
    };
    state.core.request_publish();
    Vec::new()
}

/// The tag a Markdown link names, when it is a Q-tag link.
pub fn link_tag(href: &str) -> Option<&str> {
    href.trim()
        .strip_prefix(crate::transcript::open_questions::QUESTION_LINK_PREFIX)
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_steps_back_from_the_newest_and_wraps() {
        assert_eq!(step(3, None), 2, "the first click goes to the newest");
        assert_eq!(step(3, Some(2)), 1);
        assert_eq!(step(3, Some(1)), 0);
        assert_eq!(
            step(3, Some(0)),
            2,
            "after the oldest it wraps to the newest"
        );
    }

    #[test]
    fn the_pill_shows_only_while_the_setting_is_on() {
        let mut state = ChatState::default();
        state.transcript_view.askers.push(Asker {
            id: "form".to_string(),
            kind: AskerKind::Form,
            message_id: "a".to_string(),
            tags: vec!["Q1".to_string()],
            answered: false,
            obsolete: false,
        });
        assert_eq!(project(&state).count, 0, "off by default");
        state.core.previous_question_enabled = true;
        let pill = project(&state);
        assert_eq!(pill.count, 1);
        assert_eq!(pill.text, "Previous question \u{2191}");
    }

    #[test]
    fn a_question_link_names_its_tag() {
        assert_eq!(link_tag("#gx-question=Q80"), Some("Q80"));
        assert_eq!(link_tag("https://example.com"), None);
    }
}
