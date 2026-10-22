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
use crate::transcript::page_answers::PageAnswers;

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
    /// The newest unanswered form's or page's id, or `null` when none waits. The renderers ping the
    /// pill when this changes without `count` dropping: a new question arrived.
    pub newest: Option<String>,
    /// The last jump, or `null` before the first.
    pub jump: Option<OpenQuestionJump>,
}

/// One jump to a question: scroll to `item` once per `revision`, then highlight `target` (the
/// form's id or the page's id).
///
/// CDXC:SessionChat 2026-10-20 WHY:
/// Where a jump lands, on every renderer: a reply that fits on screen is shown whole; a taller one has its end at the bottom of the transcript, unless that leaves the question above the top, and then the question's top sits `questionJumpMargin` (`scroll-bottom.json`) below the transcript's top. The question is the form question asking `question` when the form shows it, else the whole form or page card. The renderers measure the card inside the row after the row is drawn, so long unmeasured rows above it do not matter. Supersedes always aligning a tall reply's end, which left a form near the top of a long reply above the screen (user: "because im being scrolled to the bottom of the message and its sometimes loing").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestionJump {
    pub revision: u64,
    pub item: usize,
    pub target: String,
    /// The tag a Q-tag link named, so the jump lands on that question inside the form; `null` for
    /// the pill's jumps, which land on the card.
    pub question: Option<String>,
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

/// The forms and pages whose answers are on their way but not in the transcript yet: a form whose
/// answers are sending or queued (as `unanswered` reads it), and a page or form whose answers wait
/// in the prompt queue. Their Q-tags read answered already, as the pill already skips them.
pub fn answers_on_the_way(state: &ChatState) -> Vec<PageAnswers> {
    let mut found: Vec<PageAnswers> = state
        .questions
        .forms
        .iter()
        .filter_map(|(id, form)| {
            let answers = form
                .send
                .as_ref()
                .map(|send| &send.answers)
                .or(form.sent.as_ref())?;
            Some(PageAnswers {
                page_id: id.clone(),
                title: String::new(),
                answers: answers.clone(),
            })
        })
        .collect();
    for entry in state.session.queue_prompts.iter().flatten() {
        let text = entry["prompt"]["text"]
            .as_str()
            .or_else(|| entry["text"].as_str())
            .unwrap_or_default();
        if !text.contains("<ghostex-page-answers") {
            continue;
        }
        if let Some((answers, _)) = crate::transcript::page_answers::split_page_answers(text) {
            if !found.iter().any(|known| known.page_id == answers.page_id) {
                found.push(answers);
            }
        }
    }
    found
}

/// The document's `openQuestions`.
pub fn project(state: &ChatState) -> OpenQuestions {
    let open = if state.core.previous_question_enabled {
        unanswered(state)
    } else {
        Vec::new()
    };
    let count = open.len() as u32;
    OpenQuestions {
        count,
        text: "Previous question \u{2191}".to_string(),
        accessibility_label: match count {
            1 => "1 unanswered question: go to it".to_string(),
            _ => format!("{count} unanswered questions: go to the previous one"),
        },
        newest: open.last().map(|asker| asker.id.clone()),
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
    jump_to(state, context, &open[step(open.len(), at)], None)
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
    // The form's own spelling of the tag, which its drawn question carries.
    let question = asker
        .tags
        .iter()
        .find(|known| known.eq_ignore_ascii_case(tag))
        .cloned();
    jump_to(state, context, &asker, question)
}

fn jump_to(
    state: &mut ChatState,
    context: &ChatContext,
    asker: &Asker,
    question: Option<String>,
) -> Vec<Effect> {
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
            question,
        }),
    };
    state.core.request_publish();
    Vec::new()
}

/// The tag a Markdown link names, when it is a Q-tag link (`#gx-question=Q80;open`).
pub fn link_tag(href: &str) -> Option<&str> {
    href.trim()
        .strip_prefix(crate::transcript::open_questions::QUESTION_LINK_PREFIX)
        .map(|rest| rest.split(';').next().unwrap_or_default().trim())
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
            withdrawn: Vec::new(),
            keys: Vec::new(),
            skipped: Vec::new(),
            superseded: Vec::new(),
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
        assert_eq!(link_tag("#gx-question=Q80;answered"), Some("Q80"));
        assert_eq!(link_tag("https://example.com"), None);
    }
}
