//! What a finished turn's work fold says while it is closed, and what each tool row reports on its
//! right: the counts beside "Worked for 2m 5s", every call's duration, and a failed call's reason.
//!
//! CDXC:SessionChat 2026-10-18 DECISION: User picked the "proposed" work and reply look (docs/2026-10-10/chat-cards-questions/06-work.html): "Worked for 2m 5s" carries its counts beside it (tool calls, files; the failure chip was dropped 2026-10-12, see `work_stats`) so the closed fold still tells the story; tool rows get a check or cross and their duration on the right, and a failed call is tinted and says what failed. The same day the user took back the design's labelled reply toolbar (Copy · Annotate · Save): a reply's actions stay the icon-only buttons shown on hover. The GPUI chat (`completed_work_row.rs`, `tool_run.rs`) and the phone (`TranscriptItemRow.tsx`, `ToolRows.tsx`) draw what this file decides. User (2026-10-18): "Say Worked for 12s not Worked 12s. this is in all places we mention how long we worked please", superseding the design's "Worked 2m 5s" wording (`turns::worked_duration_label`).

use std::collections::HashMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage};
use serde_json::{json, Value};

use crate::transcript::background_rows::background_task_source;
use crate::transcript::jsstr::js_trim;
use crate::transcript::tool_fold::{pair_tool_blocks, split_blocks};
use crate::transcript::tool_summary::truncate_tool_preview;

/// How long a failed call's reason may run beside its preview.
const FAILURE_NOTE_LENGTH: usize = 60;

/// How far past a message the scan for its calls' results looks before it gives up.
const RESULT_SCAN_LIMIT: usize = 400;

/// "0.1s", "14.3s", "2m 5s", "1h 4m": how long one call ran.
pub fn tool_duration_label(ms: i64) -> String {
    if ms < 60_000 {
        return format!("{:.1}s", ms as f64 / 1000.0);
    }
    let seconds = (ms as f64 / 1000.0).round() as i64;
    if seconds < 3600 {
        let (minutes, rest) = (seconds / 60, seconds % 60);
        return if rest > 0 {
            format!("{minutes}m {rest}s")
        } else {
            format!("{minutes}m")
        };
    }
    let (hours, minutes) = (seconds / 3600, (seconds % 3600) / 60);
    if minutes > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{hours}h")
    }
}

/// How long each of `message`'s calls ran, by call id: from the row that recorded the call to the
/// row that recorded its result.
///
/// The transcript folds a run of tool-only rows into the message before them, so the merged
/// message's blocks no longer say when each was written; the times come from the rows as they
/// arrived, in `timeline` (the session's messages, or a deferred turn's rows). A call whose agent
/// records no call id, or whose result is not in yet, has no duration.
pub fn tool_durations<'a>(
    message: &ChatMessage,
    timelines: impl IntoIterator<Item = &'a [ChatMessage]>,
) -> HashMap<String, i64> {
    let mut wanted: HashMap<&str, (Option<i64>, Option<i64>)> = HashMap::new();
    for block in &message.blocks {
        if let ChatBlock::ToolCall {
            call_id: Some(id), ..
        } = block
        {
            wanted.entry(id.as_str()).or_default();
        }
    }
    if wanted.is_empty() {
        return HashMap::new();
    }
    for timeline in timelines {
        let Some(start) = timeline.iter().position(|row| row.id == message.id) else {
            continue;
        };
        let mut open = wanted.len();
        for row in timeline[start..].iter().take(RESULT_SCAN_LIMIT) {
            for block in &row.blocks {
                match block {
                    ChatBlock::ToolCall {
                        call_id: Some(id), ..
                    } => {
                        if let Some(times) = wanted.get_mut(id.as_str()) {
                            times.0 = times.0.or(row.timestamp);
                        }
                    }
                    ChatBlock::ToolResult {
                        call_id: Some(id), ..
                    } => {
                        if let Some(times) = wanted.get_mut(id.as_str()) {
                            if times.1.is_none() && row.timestamp.is_some() {
                                times.1 = row.timestamp;
                                open -= 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if open == 0 {
                break;
            }
        }
        break;
    }
    wanted
        .into_iter()
        .filter_map(|(id, (called, answered))| {
            let ms = answered? - called.or(message.timestamp)?;
            // Two rows stamped with the same instant say nothing about how long the call ran.
            (ms > 0).then(|| (id.to_string(), ms))
        })
        .collect()
}

/// What a failed call's row says after its preview: the first line of what the tool reported,
/// without the harness's own error wrapper.
pub fn failure_note(output: &str) -> String {
    let line = output
        .lines()
        .map(|line| {
            js_trim(
                line.trim_start_matches("<tool_use_error>")
                    .trim_end_matches("</tool_use_error>"),
            )
        })
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    truncate_tool_preview(line, FAILURE_NOTE_LENGTH)
}

/// The counts beside a finished turn's "Worked for 2m 5s": its tool calls and the files it changed.
///
/// CDXC:SessionChat 2026-10-12 DECISION: User: "no need to show "Failed" here" (the "1 failed" chip beside "Worked 5m 19s"). The fold counts only tool calls and files; a failed call still shows its cross and reason on its own row once the fold is opened. Supersedes the 2026-10-11 design's failure chip ("a failure that was fixed").
///
/// Each entry is `{kind, label}`; `kind` is `tools` or `files`, which the renderers turn into the
/// chip's glyph. A turn whose rows are still on the server (`deferredWork`) knows only its files
/// until it is opened.
pub fn work_stats(rows: &[&ChatMessage], changed_files: usize) -> Vec<Value> {
    let calls: usize = rows
        .iter()
        .map(|message| {
            // A background task's end counts as the call it reports (`background_rows.rs`).
            let message = background_task_source(message);
            let (_, tools) = split_blocks(&message.blocks);
            pair_tool_blocks(tools)
                .iter()
                .filter(|pair| pair.call_name().is_some() && pair.call_input().is_some())
                .count()
        })
        .sum();
    let mut stats = Vec::new();
    if calls > 0 {
        stats.push(json!({
            "kind": "tools",
            "label": format!("{calls} tool {}", if calls == 1 { "call" } else { "calls" }),
        }));
    }
    if changed_files > 0 {
        stats.push(json!({
            "kind": "files",
            "label": format!("{changed_files} {}", if changed_files == 1 { "file" } else { "files" }),
        }));
    }
    stats
}
