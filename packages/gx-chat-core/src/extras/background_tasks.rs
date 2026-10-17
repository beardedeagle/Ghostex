//! The rows a `shells-running` activity draws: one per background task the agent left running.
//!
//! CDXC:SessionChat 2026-10-17 DECISION:
//! User (2026-10-15): background tasks become a pill ("Background" with the running count, after Threads, Tasks and Subagents; extras/chat_pills.rs) that shows what the card showed: one row per task with its clock and (i), "+N more", the command on hover or long-press. This supersedes the card above the composer and its folded "Background tasks" row below. User (2026-10-17, Q256 and Q260): the pill rides on the working card at the transcript's end, also while the chat is idle, and the rows are its section in the open card (extras/working_card.rs) instead of a floating menu (a sheet on the phone). The rest of the 2026-10-11 decision stands, as follows. User (2026-10-11): "1 shell still running" was not clear enough for non-technical users. From the mockups they picked "M3" for every count, also one task: one row per task with its own timer, up to three rows and then "+N more". There is no heading line over the rows: "Still running in the background" sits in an (i) left of each row's timer, a tooltip on hover (desktop) and shown on tap or long-press (phone). While a turn runs, the working indicator stays and the Background pill sits beside it, never a replacement (see `working_strip`). A row is titled by the description the agent gave the command, a monitor reads "Watching …", and the command itself shows on hover (desktop) or long-press (phone). When a task ends, the chat's existing finished row keeps its look; the user later dropped the duration it first gained ("remove exit code 0 and "after 39s" here ... useless info", see `transcript::noise::split_status_summary`).
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! server/src/session_chat_claude_background.rs reads the tasks from Claude's transcript; apps/desktop/src/app/native_chat/working_strip.rs and apps/mobile/app/src/chat/native/cards/WorkingStrip.tsx draw these rows and must match.

use serde_json::{json, Value};

use crate::extras::activity::format_activity_elapsed;
use crate::extras::time::parse_iso_millis;

/// Rows shown before the rest fold into "+N more".
pub const MAX_BACKGROUND_TASK_ROWS: usize = 3;

/// Whether the activity names background tasks whose clocks move.
pub fn has_background_task_clock(activity: &Value) -> bool {
    activity
        .pointer("/background/tasks")
        .and_then(Value::as_array)
        .is_some_and(|tasks| !tasks.is_empty())
}

/// `{ info, rows: [{ id, prefix, title, command, elapsed }], more, grouped, title, meta }` for the
/// strip, `info` being what each row's (i) says and `grouped` whether two or more tasks fold under
/// one "Background tasks" row (`title`, with `meta` its muted count), or `None` when the activity
/// carries no `background` (another kind, or a gxserver that predates the field, whose strip keeps
/// the footer's own label).
pub fn background_tasks(activity: &Value, now_ms: f64, utc_offset_minutes: i32) -> Option<Value> {
    let background = activity.get("background")?.as_object()?;
    let tasks: Vec<&Value> = background
        .get("tasks")
        .and_then(Value::as_array)
        .map(|tasks| tasks.iter().collect())
        .unwrap_or_default();
    let count = background
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .max(tasks.len() as u64);
    let info = "Still running in the background";
    let rows: Vec<Value> = tasks
        .iter()
        .take(MAX_BACKGROUND_TASK_ROWS)
        .map(|task| {
            let started = task
                .get("startedAt")
                .and_then(Value::as_str)
                .and_then(|at| parse_iso_millis(at, utc_offset_minutes));
            json!({
                "id": task.get("id").cloned().unwrap_or(Value::Null),
                "prefix": (task.get("kind").and_then(Value::as_str) == Some("monitor"))
                    .then_some("Watching"),
                "title": task.get("title").and_then(Value::as_str).unwrap_or_default(),
                "command": task.get("command").and_then(Value::as_str),
                "elapsed": started
                    .map(|started| format_activity_elapsed(((now_ms - started) / 1_000.0).max(0.0))),
            })
        })
        .collect();
    let hidden = count.saturating_sub(rows.len() as u64);
    let more = match (rows.is_empty(), hidden) {
        (_, 0) => None,
        // Nothing named: the transcript did not record what was started, so only the count is known,
        // and with no row to carry the (i) the line says it itself.
        (true, 1) => Some("1 command still running in the background".to_string()),
        (true, n) => Some(format!("{n} commands still running in the background")),
        (false, n) => Some(format!("+{n} more")),
    };
    Some(json!({
        "info": info,
        "rows": rows,
        "more": more,
        "grouped": count >= 2,
        "title": "Background tasks",
        "meta": format!("{count} running"),
        "count": count,
    }))
}
