//! The tool call an approval is waiting on, opened in the transcript so its command shows beside
//! the approval card.
//!
//! SEE-ALSO: `apps/desktop/src/app/native_chat/tool_run.rs` and
//! `apps/mobile/app/src/chat/native/transcript/ToolRows.tsx` read `awaitingApproval` as the row's
//! open default, and record a reader's own open or close against it.

use serde_json::Value;

use crate::document::TranscriptItem;
use crate::questions::gates::{notice_visible, prompt_key};
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_state::notice_dismiss_key;
use crate::state::ChatState;
use crate::transcript::presentation::{message_tool_rows, ProjectionCache};
use crate::transcript::tool_fold::{pair_tool_blocks, split_blocks, ToolPair};
use crate::transcript::tool_rows::tool_fold_label;

/// The agent is blocked on a tool approval. `tool_use_id` names the call when gxserver knew it
/// (a hook approval); a screen permission dialog alone names none, and the newest call still
/// waiting for its result is the one it asks about.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalWait {
    pub tool_use_id: Option<String>,
}

/// The approval the agent is blocked on, while its card is still up.
///
/// A screen permission dialog decides when there is one: its rows are the card, and answering it
/// (here, in the terminal, or anywhere else) takes the wait away. Without one the hook approval
/// decides, until its answer goes out.
pub fn approval_wait(state: &ChatState) -> Option<ApprovalWait> {
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    let approval = matches!(prompt, Some(InteractivePrompt::Approval { .. }));
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref())
        .filter(|notice| notice.kind == "permissionPrompt");
    let waiting = match &notice {
        Some(notice) => {
            notice_visible(state)
                && notice_dismiss_key(Some(notice)) != state.questions.retired_notice_key
        }
        None => approval && prompt_key(prompt.as_ref()) != state.questions.dismissed_prompt,
    };
    waiting.then(|| ApprovalWait {
        tool_use_id: prompt
            .as_ref()
            .filter(|_| approval)
            .and_then(InteractivePrompt::tool_use_id)
            .map(str::to_string),
    })
}

/// Where the waiting call is drawn: the item, the row inside a summary turn's work, and the call's
/// index in that row's `tools`.
struct Target {
    item: usize,
    work: Option<usize>,
    tool: usize,
}

/// What one projected row says about the waiting call.
enum Search {
    /// The call is this row's tool at that index.
    Found(usize),
    /// The call is in this row but drawn as something other than a tool row (a file card).
    Elsewhere,
    NotHere,
}

/// The tool row index of `call` among the message's tool rows, or `Elsewhere` when the call is a
/// file change the row draws as a file card.
fn tool_row_of(
    source: &ghostex_gx_protocol::ChatMessage,
    call: &ghostex_gx_protocol::ChatBlock,
) -> Search {
    let (_, rows) = message_tool_rows(source);
    match rows.iter().position(|pair| {
        pair.call
            .is_some_and(|row_call| std::ptr::eq(row_call, call))
    }) {
        Some(index) => Search::Found(index),
        None => Search::Elsewhere,
    }
}

fn calls(source: &ghostex_gx_protocol::ChatMessage) -> Vec<ToolPair<'_>> {
    let (_, tools) = split_blocks(&source.blocks);
    pair_tool_blocks(tools)
}

/// The call named `id` in this row.
fn by_id(source: &ghostex_gx_protocol::ChatMessage, id: &str) -> Search {
    let found = calls(source).into_iter().find_map(|pair| match pair.call {
        Some(
            call @ ghostex_gx_protocol::ChatBlock::ToolCall {
                call_id: Some(call_id),
                ..
            },
        ) if call_id == id => Some(call),
        _ => None,
    });
    match found {
        Some(call) => tool_row_of(source, call),
        None => Search::NotHere,
    }
}

/// The newest call of this row still waiting for its result, when the row made any call at all.
fn newest_open_call(source: &ghostex_gx_protocol::ChatMessage) -> Search {
    let pairs = calls(source);
    if !pairs.iter().any(|pair| pair.call.is_some()) {
        return Search::NotHere;
    }
    match pairs
        .iter()
        .rev()
        .find(|pair| pair.call.is_some() && pair.result.is_none())
        .and_then(|pair| pair.call)
    {
        Some(call) => tool_row_of(source, call),
        // The newest row that called anything has every answer back: the waiting call is not in
        // the transcript yet, and an older finished call is not what the approval asks about.
        None => Search::Elsewhere,
    }
}

/// The rows the list draws, newest first, each with where it sits.
fn rows_newest_first(items: &[TranscriptItem]) -> Vec<(usize, Option<usize>, &Value)> {
    let mut rows = Vec::new();
    for (item_index, item) in items.iter().enumerate().rev() {
        match item {
            TranscriptItem::Message { message } => rows.push((item_index, None, message)),
            TranscriptItem::Summary { work, .. } => {
                for (work_index, message) in work.iter().enumerate().rev() {
                    rows.push((item_index, Some(work_index), message));
                }
            }
            // A finished turn's work holds no call an approval still waits on.
            TranscriptItem::CompletedWork { .. } | TranscriptItem::Unknown(_) => {}
        }
    }
    rows
}

fn locate(
    items: &[TranscriptItem],
    cache: &ProjectionCache,
    wait: &ApprovalWait,
) -> Option<Target> {
    let rows = rows_newest_first(items);
    let search = |find: &dyn Fn(&ghostex_gx_protocol::ChatMessage) -> Search| {
        for (item, work, message) in &rows {
            let Some(entry) = message
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| cache.get(id))
            else {
                continue;
            };
            match find(&entry.source) {
                Search::Found(tool) => {
                    return Some(Some(Target {
                        item: *item,
                        work: *work,
                        tool,
                    }))
                }
                Search::Elsewhere => return Some(None),
                Search::NotHere => {}
            }
        }
        None
    };
    if let Some(id) = &wait.tool_use_id {
        // Claude's hook names the transcript's own call id; an agent whose approval id names no
        // call (OpenCode's permission ids) is matched like a screen dialog.
        if let Some(found) = search(&|source| by_id(source, id)) {
            return found;
        }
    }
    search(&newest_open_call).flatten()
}

/// CDXC:SessionChat 2026-10-15 DECISION: User (a Ghostex customer): "when I run Claude Code in manual Mode I always have to expand the bash commands to see what ghostex is asking permission for. (in chat view) It would be better if we expand to show the last command so i know what it needs approval for. (collapse after i approve)". While an agent waits on a tool approval, the call it waits on starts open (`awaitingApproval` on the tool row, `toolsOpen` on its message, and the run fold and a summary turn's work fold keep it in view), so the command shows beside the approval card without a click. Once the approval is answered, from the chat, the terminal or anywhere, the mark goes and the row folds back to how it normally draws; a row the reader opened or closed by hand while it waited keeps their choice, because the renderers record that against this default.
pub fn open_waiting_call(
    items: &mut [TranscriptItem],
    cache: &ProjectionCache,
    wait: Option<&ApprovalWait>,
) {
    let Some(wait) = wait else { return };
    let Some(target) = locate(items, cache, wait) else {
        return;
    };
    let message = match (&mut items[target.item], target.work) {
        (TranscriptItem::Message { message }, None) => message,
        (
            TranscriptItem::Summary {
                work, work_open, ..
            },
            Some(index),
        ) => {
            *work_open = true;
            &mut work[index]
        }
        _ => return,
    };
    let Value::Object(fields) = message else {
        return;
    };
    fields.insert("toolsOpen".to_string(), Value::Bool(true));
    if let Some(Value::Object(tool)) = fields
        .get_mut("tools")
        .and_then(|tools| tools.get_mut(target.tool))
    {
        tool.insert("awaitingApproval".to_string(), Value::Bool(true));
    }
    // The "+N previous tool calls" fold keeps only the last call in view; a waiting call earlier in
    // the run (parallel calls) stays out of it.
    if let Some(Value::Object(fold)) = fields.get_mut("toolFold") {
        let hidden = fold
            .get("visible")
            .and_then(|visible| visible.get(target.tool))
            .is_some_and(|shown| shown == &Value::Bool(false));
        if hidden {
            if let Some(Value::Array(visible)) = fold.get_mut("visible") {
                visible[target.tool] = Value::Bool(true);
            }
            let count = fold
                .get("hiddenCount")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .saturating_sub(1);
            fold.insert("hiddenCount".to_string(), count.into());
            fold.insert(
                "collapsedLabel".to_string(),
                tool_fold_label(count as usize).into(),
            );
        }
    }
}
