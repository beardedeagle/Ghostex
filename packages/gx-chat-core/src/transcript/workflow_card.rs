//! The workflow card: one Claude Code Workflow call (a script that starts many agents in phases)
//! drawn as the run it started rather than as a plain tool row.
//!
//! gxserver reads the run off disk and sends it as the `workflows` side state
//! (`server/src/session_chat_claude_workflow_cards.rs`); this file turns it into the card both
//! renderers lay out, keyed by the Workflow call's id. The transcript marks the call's tool row
//! (`workflow`), the message that made it (`workflows`) and a finished turn's fold (`workflows`,
//! hoisted like answered questions), so the card is drawn once, outside any fold, and a call whose
//! run gxserver has not read yet stays an ordinary tool row.
//!
//! CDXC:SessionChat 2026-10-22 DECISION: User asked for a nice UI for Claude Code's workflows in the chat, "using the same cards look that we have now in our chat": a "Workflow started" header with the workflow's name and description and a summary on the right ("4 agents · 2 working · 2 done"), the phases as small headings with their progress ("Map 2/2"), each phase's one-line description from the script (`meta.phases[].detail`, shipped as the phase's `detail`) as a muted line under its heading (the user's answer to Q383, "Show it under the heading", superseding the first build's hover tooltip), one row per agent (avatar with a status dot, label, a one-line preview of its output or prompt, its duration, a chevron that opens it in the subagent viewer), then the workflow's script in a code block collapsed behind "Show full script". Counts, statuses and durations update live while it runs. The desktop (`apps/desktop/src/app/native_chat/workflow_card.rs`) and the phone (`apps/mobile/app/src/chat/native/transcript/WorkflowCard.tsx`) draw the same card.

use std::collections::BTreeMap;

use ghostex_gx_protocol::ChatBlock;
use serde_json::{json, Value};

use crate::state::{ChatContext, ChatState};
use crate::transcript::tool_fold::ToolPair;

/// The tool Claude Code runs a workflow with.
pub const WORKFLOW_TOOL_NAME: &str = "Workflow";

/// How many of the script's lines the collapsed code block shows.
pub const WORKFLOW_SCRIPT_PREVIEW_LINES: usize = 6;

/// The Workflow call a tool pair is, by its call id.
pub fn workflow_call_id(pair: &ToolPair<'_>) -> Option<String> {
    match pair.call {
        Some(ChatBlock::ToolCall {
            name,
            call_id: Some(id),
            ..
        }) if name == WORKFLOW_TOOL_NAME && !id.is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// "45s", "5m 06s", "1h 04m": how long an agent or the run has been going.
pub fn workflow_duration_label(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3600 {
        return format!("{}m {:02}s", seconds / 60, seconds % 60);
    }
    format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The cards' summary line: "4 agents · 2 working · 2 done".
fn summary_label(agents: &[&Value]) -> String {
    let count = |status: &str| {
        agents
            .iter()
            .filter(|agent| str_of(agent, "status") == status)
            .count()
    };
    let mut parts = vec![plural(agents.len(), "agent", "agents")];
    for (status, label) in [
        ("queued", "waiting"),
        ("running", "working"),
        ("done", "done"),
        ("failed", "failed"),
        ("stopped", "stopped"),
    ] {
        let n = count(status);
        if n > 0 {
            parts.push(format!("{n} {label}"));
        }
    }
    parts.join(" \u{b7} ")
}

/// The script as a fenced `js` block through the transcript's own Markdown pass, so it gets the
/// code block every reply's code gets (its header, copy and wrap).
fn script_markdown(script: &str) -> String {
    if script.is_empty() {
        return String::new();
    }
    // A fence longer than any run of backticks inside the script, which can hold template strings.
    let longest = script
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    crate::transcript::native_markdown::native_markdown(
        &format!("{fence}js\n{script}\n{fence}"),
        false,
    )
}

/// One agent row, and whether its clock is still moving.
fn agent_row(agent: &Value, run_running: bool, now: i64, index: usize) -> (Value, bool) {
    let status = match str_of(agent, "status") {
        "running" if !run_running => "stopped",
        "" => "running",
        other => other,
    };
    let started = agent.get("startedAt").and_then(Value::as_i64);
    let finished = agent.get("finishedAt").and_then(Value::as_i64);
    let ticking = status == "running" && started.is_some();
    let duration = match (started, finished) {
        (Some(start), Some(end)) => workflow_duration_label(end - start),
        (Some(start), None) if ticking => workflow_duration_label(now - start),
        _ => String::new(),
    };
    let id = str_of(agent, "id");
    let label = str_of(agent, "label");
    let status_label = match status {
        "queued" => "Waiting",
        "running" => "Working",
        "done" => "Done",
        "failed" => "Failed",
        _ => "Stopped",
    };
    (
        json!({
            "key": if id.is_empty() { format!("agent-{index}") } else { id.to_string() },
            // What the subagent viewer is opened with; empty for an agent that never started.
            "selector": id,
            "label": if label.is_empty() { id } else { label },
            "preview": str_of(agent, "preview"),
            "status": status,
            "statusLabel": status_label,
            "durationLabel": duration,
            "model": str_of(agent, "model"),
        }),
        ticking,
    )
}

/// One run's card, and whether a clock on it is still moving.
fn card(run: &Value, now: i64) -> (Value, bool) {
    let status = match str_of(run, "status") {
        "" => "running",
        other => other,
    };
    let running = status == "running";
    let agents: Vec<&Value> = run
        .get("agents")
        .and_then(Value::as_array)
        .map(|list| list.iter().collect())
        .unwrap_or_default();
    let mut rows: Vec<(String, Value)> = Vec::new();
    let mut ticking = false;
    let mut shown_agents: Vec<Value> = Vec::new();
    for (index, agent) in agents.iter().enumerate() {
        let (row, moving) = agent_row(agent, running, now, index);
        ticking |= moving;
        rows.push((str_of(agent, "phase").to_string(), row.clone()));
        shown_agents.push(row);
    }
    let shown: Vec<&Value> = shown_agents.iter().collect();
    let listed: Vec<&Value> = run
        .get("phases")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|phase| !str_of(phase, "title").is_empty())
                .collect()
        })
        .unwrap_or_default();
    let known = |phase: &str| listed.iter().any(|listed| str_of(listed, "title") == phase);
    let mut groups: Vec<(&str, &str, Vec<Value>)> = listed
        .iter()
        .map(|phase| {
            let title = str_of(phase, "title");
            let members = rows
                .iter()
                .filter(|(agent_phase, _)| agent_phase == title)
                .map(|(_, row)| row.clone())
                .collect();
            (title, str_of(phase, "detail"), members)
        })
        .collect();
    // Agents that name no phase the run listed are grouped last, under no heading.
    let others: Vec<Value> = rows
        .iter()
        .filter(|(phase, _)| !known(phase))
        .map(|(_, row)| row.clone())
        .collect();
    if !others.is_empty() {
        groups.push(("", "", others));
    }
    let phases: Vec<Value> = groups
        .into_iter()
        .map(|(title, detail, members)| {
            let done = members
                .iter()
                .filter(|row| str_of(row, "status") == "done")
                .count();
            json!({
                "key": if title.is_empty() { "other".to_string() } else { format!("phase:{title}") },
                "title": title,
                // The script's one-line description of the phase, drawn under its heading.
            "detail": detail,
                // A phase whose agents have not started yet has nothing to count.
                "countLabel": if members.is_empty() { String::new() } else { format!("{done}/{}", members.len()) },
                "agents": members,
            })
        })
        .collect();
    let script = str_of(run, "script").trim_end().to_string();
    let script_lines = script.lines().count();
    let heading = match status {
        "running" => "Workflow started",
        "completed" => "Workflow finished",
        "failed" => "Workflow failed",
        _ => "Workflow stopped",
    };
    let started = run.get("startedAt").and_then(Value::as_i64);
    let finished = run.get("finishedAt").and_then(Value::as_i64);
    let duration = match (started, finished) {
        (Some(start), Some(end)) => workflow_duration_label(end - start),
        (Some(start), None) if running => workflow_duration_label(now - start),
        _ => String::new(),
    };
    (
        json!({
            "callId": str_of(run, "callId"),
            "runId": str_of(run, "runId"),
            "status": status,
            "heading": heading,
            "name": str_of(run, "name"),
            "description": str_of(run, "description"),
            "summary": summary_label(&shown),
            "durationLabel": duration,
            "phases": phases,
            "script": script,
            "scriptMarkdown": script_markdown(&script),
            // The collapsed code block shows this many lines and offers the rest behind the label.
            "scriptPreviewLines": WORKFLOW_SCRIPT_PREVIEW_LINES,
            "scriptCollapsible": script_lines > WORKFLOW_SCRIPT_PREVIEW_LINES,
            "showScriptLabel": "Show full script",
            "hideScriptLabel": "Show less",
        }),
        running && ticking,
    )
}

/// Every workflow card the session's runs make, by Workflow call id, and whether a clock on one
/// is still moving (the host then re-projects once a second, on the Subagents strip's clock).
pub fn workflow_cards(state: &ChatState, context: &ChatContext) -> (BTreeMap<String, Value>, bool) {
    let now = context.now_ms as i64;
    let mut cards = BTreeMap::new();
    let mut ticking = false;
    for run in state
        .session
        .workflows
        .as_ref()
        .and_then(|workflows| workflows.get("runs"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let call_id = str_of(run, "callId");
        if call_id.is_empty() {
            continue;
        }
        let (card, moving) = card(run, now);
        ticking |= moving;
        cards.insert(call_id.to_string(), card);
    }
    (cards, ticking)
}
