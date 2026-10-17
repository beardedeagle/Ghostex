//! The working card: one card at the end of the transcript that says what the agent is doing, with
//! the pills beside its label, and opens to the running tool's call and a section per pill.
//!
//! CDXC:SessionChat 2026-10-17 DECISION:
//! User, answering the working indicator review (docs/2026-10-17/chat-working-indicator/review.html): the pending tool card and the "✦ Considering…" working row with its pills become one card ("card", Q253), the transcript's last item when the reader is at the bottom, instead of a row above the composer, so the chat stops shifting. Scrolled up, the same card floats over the transcript's bottom edge (Q255, recommended A), so the chat never moves, and it must be "the same size and look as the one shown at the bottom of the transcript" with a smooth change between the two and nothing shifting. While a tool runs its label replaces the working word, which comes back between tools ("tool label", Q254). The pills are a summary and opening the card shows every section inline (Q256, recommended A); the ? question pill and an armed Delayed Send ride on the card ("on the pill", Q259); idle pills (Threads on an orchestrator, Background after a turn) stay in the lane ("in the lane", Q260). When the turn ends the lane shows "Worked 2m 13s" until the next message ("worked line", Q257) and that turn's fold drops its time (`transcript::rows::without_worked_time`); an open card closes with an ease ("closes", Q258). The phone opens it with a tap ("tap", Q261). Later the same day: "let's not have that chevron at all, it's not needed" (a click or tap on the card opens it and nothing is drawn for a chevron); "make the scroll to bottom pill always appear on the leftmost so it doesn't push other pills when it appears"; "the text for the working pill should be hoverable to see full text of it"; and "shouldn't allow wrapping in the pill itself, we should truncate the text on the left to make space for the pills (... at the end)": the label stays on one line, cut with an ellipsis at its end, and the pills keep their full width. Agents with no status card (no tool read off their screen) show the card with the spinner text alone. Supersedes the 2026-09-16 rule that the working strip stays a bare line and never becomes a card (cards.rs), the 2026-10-15 pills on the working row above the composer (`chat_pills`), the 2026-10-11 order of the row above the composer (the working indicator first, then the pills, then the compaction card, in the deleted working_strip.rs: the compaction now is the card's label with its progress beside it) and the pending tool card's own row at the transcript's tail (`session::composition`).
//!
//! CDXC:SessionChat 2026-10-17 SEE-ALSO:
//! apps/desktop/src/app/native_chat/working_card.rs (desktop and web) and apps/mobile/app/src/chat/native/cards/WorkingCard.tsx (phone) draw this value and must match.

use ghostex_gx_protocol::{ChatBlock, ChatRole};
use serde_json::{json, Value};

use crate::document::{ChatPill, WorkingCard, WorkingStrip};
use crate::session::terminal::visible_terminal_tool;
use crate::state::ChatState;
use crate::transcript::foreign::terminal_tool_activity;
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_rows::tool_preview;
use crate::transcript::tool_summary::tool_description;

/// The card's projection from the working strip and the pills the document already holds.
pub fn working_card(
    state: &ChatState,
    strip: &WorkingStrip,
    pills: &[ChatPill],
    question_waiting: bool,
) -> WorkingCard {
    let working = state.session.server_working || state.session.external_working;
    let tool = visible_terminal_tool(state, crate::session::working::is_working(state))
        .map(terminal_tool_activity)
        .filter(|tool| {
            tool.get("label")
                .and_then(Value::as_str)
                .is_some_and(|label| !label.is_empty())
        });
    let running = (working && tool.is_none())
        .then(|| running_call_label(state))
        .flatten();
    let activity = strip.presentation.clone().filter(Value::is_object);
    let worked = state
        .transcript_view
        .worked_line
        .clone()
        .filter(|_| !working);
    let (label, label_kind) = if let Some(activity) = &activity {
        (
            activity
                .get("label")
                .and_then(Value::as_str)
                .map(str::to_string),
            "activity",
        )
    } else if let Some(tool) = &tool {
        (
            tool.get("label")
                .and_then(Value::as_str)
                .map(str::to_string),
            "tool",
        )
    } else if let Some(label) = running {
        (Some(label), "tool")
    } else if let Some(word) = &strip.label {
        (Some(word.clone()), "word")
    } else {
        (worked.clone(), "worked")
    };
    // One line: the renderers cut it with an ellipsis and show the whole text on hover.
    let label = label
        .map(|label| label.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|label| !label.is_empty());
    let label_kind = label.as_ref().map(|_| label_kind.to_string());
    let tool_section = tool.as_ref().and_then(|tool| {
        let detail = tool.get("detail").and_then(Value::as_str)?.trim();
        (!detail.is_empty()).then(|| {
            json!({
                "title": "Running now",
                "label": tool.get("label").cloned().unwrap_or(Value::Null),
                "detail": detail,
            })
        })
    });
    let sections: Vec<String> = pills.iter().map(|pill| pill.id.clone()).collect();
    let expandable = tool_section.is_some() || !sections.is_empty();
    let phase = if working {
        "working"
    } else if label_kind.as_deref() == Some("worked") {
        "worked"
    } else {
        "idle"
    };
    let aria_label = label
        .iter()
        .cloned()
        .chain(pills.iter().map(|pill| pill.aria_label.clone()))
        .collect::<Vec<_>>()
        .join(", ");
    WorkingCard {
        visible: label.is_some() || !sections.is_empty() || question_waiting,
        phase: phase.to_string(),
        working,
        label,
        label_kind,
        activity,
        tool: tool_section,
        sections,
        expandable,
        open: expandable && state.extras.panels.working_card_open,
        aria_label,
    }
}

/// The label of the call the transcript records as still running in the current turn: its
/// description, else its name and command. The tool row read off the screen retires once the
/// transcript records the call, and the label must stay for as long as the call runs (Q254).
fn running_call_label(state: &ChatState) -> Option<String> {
    let mut answered: Vec<&str> = Vec::new();
    for message in state.messages.list.iter().rev().take(400) {
        let mut prompt = message.role == ChatRole::User;
        for block in message.blocks.iter().rev() {
            match block {
                ChatBlock::ToolResult {
                    call_id: Some(id), ..
                } => {
                    prompt = false;
                    answered.push(id);
                }
                ChatBlock::ToolResult { .. } => prompt = false,
                ChatBlock::ToolCall {
                    name,
                    input,
                    call_id: Some(id),
                } if !answered.contains(&id.as_str()) => {
                    let description = tool_description(input);
                    if !description.is_empty() {
                        return Some(description);
                    }
                    let preview = tool_preview(
                        &ToolPair {
                            call: Some(block),
                            result: None,
                        },
                        None,
                    );
                    return Some(if preview.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}: {preview}")
                    });
                }
                _ => {}
            }
        }
        // The prompt that opened the turn: nothing before it is running.
        if prompt {
            return None;
        }
    }
    None
}

/// Opens or shuts the card. Shutting it sends the Threads section back to its first step, as
/// closing the Threads menu did (CDXC:Coordinators 2026-10-15 in coordinator_threads.rs).
pub fn set_open(state: &mut ChatState, open: bool) {
    state.extras.panels.working_card_open = open;
    if !open {
        crate::extras::coordinator_threads::set_step(&mut state.extras.panels, 0);
    }
}

/// An open card closes when the turn ends (Q258, "closes"); the renderers ease it shut.
pub fn settle(state: &mut ChatState, working: bool) {
    let panels = &mut state.extras.panels;
    if panels.working_card_last_working == Some(true) && !working && panels.working_card_open {
        panels.working_card_open = false;
        crate::extras::coordinator_threads::set_step(panels, 0);
    }
    panels.working_card_last_working = Some(working);
}
