//! `/api/readSessionChatReport`: the chat's Copy Report, a Markdown report of what Ghostex sees in
//! one session's terminal, for the user to paste into Send Bug / Feedback or a GitHub issue.

use serde_json::{json, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

/// Send problems the report lists, newest last.
const RECENT_SEND_PROBLEMS: usize = 8;
/// A logged message longer than this is cut, so one entry cannot crowd out the screen.
const PROBLEM_MESSAGE_MAX_CHARS: usize = 240;

/// CDXC:SessionChat 2026-10-25 DECISION:
/// User: "i want a way to report such issues where it goes and captures the terminal for me into clipboard when the chat composer shows red so ppl can share it… should copy full screen of terminal with 200 width basically as seen by ghostex", with "useful info" about the current state. The report is built here, on the machine that runs the session, so every chat (desktop, web, phone, a remote session) copies the same text: the session, the input-box verdict and the notice Ghostex reads off the live grid, the terminal's size and clients, the queue, the session's recent refused or held sends, and the whole live grid as gxserver captures it. It names no folders, accounts or message text beyond what is on the screen.
/// SEE-ALSO: packages/gx-chat-core/src/extras/report.rs (Copy Report: copy, toast, then Send Bug / Feedback), server/src/session_chat_composer.rs (`read_session_terminal_tail`, the same verdict for the not-ready card).
pub(crate) fn read_session_chat_report(
    state: &AppState,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Session {session_id} does not exist."))
        })?;
    let zmx_name = crate::zmx::provider_zmx_session_name(&session)?;
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
    let text_of = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let global_ref = format!("{}:{project_id}:{session_id}", state.metadata.server_id);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z");

    let mut report = String::new();
    report.push_str("## Ghostex session report\n\n");
    report.push_str(&format!(
        "Ghostex {} · {} · copied {now}\n\n",
        crate::telemetry::base::SERVER_MARKETING_VERSION,
        os_line(),
    ));

    report.push_str("### Session\n\n");
    report.push_str(&format!(
        "- Agent: {}\n",
        agent.as_deref().unwrap_or("unknown")
    ));
    report.push_str(&format!(
        "- Session: {global_ref}{}\n",
        text_of("title")
            .map(|title| format!(" \"{title}\""))
            .unwrap_or_default()
    ));
    report.push_str(&format!(
        "- State: {} · {}\n",
        crate::presentation::effective_lifecycle_state(&session),
        crate::presentation::presentation_activity(&session, &crate::presentation::now_iso()),
    ));
    let grid = crate::zmx::read_zmx_grid(&zmx_name)
        .and_then(|grid| serde_json::from_str::<Value>(&grid).ok());
    report.push_str(&format!(
        "- Terminal: {zmx_name}{}\n",
        grid_line(grid.as_ref())
    ));

    // The live grid alone, the screen Ghostex reads and the user sees, without scrollback.
    let capture = crate::zmx::read_zmx_session_grid_capture(&zmx_name);
    report.push_str("\n### What Ghostex sees\n\n");
    match capture.as_ref() {
        Ok(capture) => {
            let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
                agent.as_deref(),
                &capture.text,
            );
            let readiness = crate::session_chat_composer::detect_session_chat_composer_readiness(
                agent.as_deref(),
                &capture.text,
                notice.as_ref(),
            );
            report.push_str(&format!(
                "- Input box: {}\n",
                match readiness.state {
                    crate::session_chat_composer::SessionChatComposerState::Ready => {
                        "on screen and ready".to_string()
                    }
                    crate::session_chat_composer::SessionChatComposerState::NotReady => format!(
                        "not ready: {}",
                        readiness.reason.as_deref().unwrap_or("no reason given")
                    ),
                    crate::session_chat_composer::SessionChatComposerState::Unknown => {
                        "unknown (Ghostex has no input-box signature for this agent)".to_string()
                    }
                }
            ));
            if readiness.should_dismiss() {
                report.push_str("- A send closes the panel over the input box first.\n");
            }
            report.push_str(&format!(
                "- Terminal notice: {}\n",
                notice
                    .map(|notice| format!("{} ({})", notice.title, notice.kind))
                    .unwrap_or_else(|| "none".to_string())
            ));
        }
        Err(error) => {
            report.push_str(&format!("- Ghostex could not read the terminal: {error}\n"));
        }
    }
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot(
        &state.paths,
        project_id,
        session_id,
    );
    report.push_str(&format!("- Queue: {}\n", queue_line(&queue.queue)));

    report.push_str("\n### Recent send problems\n\n");
    let problems = recent_send_problems(&state.paths.logs_dir, project_id, session_id);
    if problems.is_empty() {
        report.push_str("None logged.\n");
    } else {
        for problem in problems {
            report.push_str(&format!("- {problem}\n"));
        }
    }

    if let Ok(capture) = capture.as_ref() {
        let screen = screen_rows(&capture.text);
        let fence = code_fence(&screen);
        report.push_str(&format!(
            "\n### Terminal screen{}\n\n{fence}text\n{screen}\n{fence}\n",
            grid.as_ref()
                .and_then(grid_size)
                .map(|(rows, cols)| format!(" ({rows} × {cols}, as Ghostex reads it)"))
                .unwrap_or_default(),
        ));
    }
    Ok(json!({ "report": report, "projectId": project_id, "sessionId": session_id }))
}

/// `macOS (aarch64)`: the platform without spawning a process for its version.
fn os_line() -> String {
    let name = match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    format!("{name} ({})", std::env::consts::ARCH)
}

fn grid_size(grid: &Value) -> Option<(u64, u64)> {
    Some((grid.get("rows")?.as_u64()?, grid.get("cols")?.as_u64()?))
}

/// ` · 24 × 200 · clients: visible 24 × 200, parked 41 × 82`.
fn grid_line(grid: Option<&Value>) -> String {
    let Some(grid) = grid else {
        return " · its size could not be read".to_string();
    };
    let mut line = grid_size(grid)
        .map(|(rows, cols)| format!(" · {rows} × {cols}"))
        .unwrap_or_default();
    let clients: Vec<String> = grid
        .get("clients")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|client| {
            let state = client
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            match client.get("last_size").and_then(grid_size) {
                Some((rows, cols)) => format!("{state} {rows} × {cols}"),
                None => state.to_string(),
            }
        })
        .collect();
    line.push_str(&if clients.is_empty() {
        " · no clients attached".to_string()
    } else {
        format!(" · clients: {}", clients.join(", "))
    });
    line
}

/// `3 waiting (2 held for the input box), 1 failed: <reason>`; message text stays out.
fn queue_line(queue: &[crate::session_chat_queue::SessionChatQueuedPrompt]) -> String {
    if queue.is_empty() {
        return "empty".to_string();
    }
    let waiting = queue.iter().filter(|row| row.state != "failed").count();
    let held = queue
        .iter()
        .filter(|row| row.state != "failed" && row.startup_send)
        .count();
    let mut parts = vec![if held > 0 {
        format!("{waiting} waiting ({held} held for the input box)")
    } else {
        format!("{waiting} waiting")
    }];
    let oldest = queue.iter().map(|row| row.created_at.as_str()).min();
    if let Some(oldest) = oldest {
        parts.push(format!("oldest from {oldest}"));
    }
    for failed in queue.iter().filter(|row| row.state == "failed") {
        parts.push(format!(
            "1 failed: {}",
            failed.error_message.as_deref().unwrap_or("no reason given")
        ));
    }
    parts.join(", ")
}

/// This session's newest entries in gxserver's send log (`session-chat-send-failures.jsonl`),
/// one line each, without the screens they carry.
fn recent_send_problems(
    logs_dir: &std::path::Path,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    let Ok(log) = std::fs::read_to_string(logs_dir.join("session-chat-send-failures.jsonl")) else {
        return Vec::new();
    };
    let mut problems: Vec<String> = log
        .lines()
        .filter(|line| line.contains(session_id))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| {
            entry.get("sessionId").and_then(Value::as_str) == Some(session_id)
                && entry.get("projectId").and_then(Value::as_str) == Some(project_id)
        })
        .map(|entry| {
            let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or_default();
            let message = [text("message"), text("reason")]
                .into_iter()
                .find(|message| !message.is_empty())
                .unwrap_or_default();
            let message: String = message.chars().take(PROBLEM_MESSAGE_MAX_CHARS).collect();
            let code = text("code");
            format!(
                "{} {}{}{}",
                text("ts"),
                text("event"),
                if code.is_empty() {
                    String::new()
                } else {
                    format!(" ({code})")
                },
                if message.is_empty() {
                    String::new()
                } else {
                    format!(": {message}")
                },
            )
        })
        .collect();
    let keep = problems.len().saturating_sub(RECENT_SEND_PROBLEMS);
    problems.drain(..keep);
    problems
}

/// The grid's rows with trailing spaces and trailing blank rows removed, and control characters
/// other than tabs dropped.
fn screen_rows(text: &str) -> String {
    let rows: Vec<String> = text
        .lines()
        .map(|row| {
            row.chars()
                .filter(|ch| !ch.is_control() || *ch == '\t')
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    let end = rows
        .iter()
        .rposition(|row| !row.is_empty())
        .map_or(0, |last| last + 1);
    rows[..end].join("\n")
}

/// A fence longer than any backtick run on the screen, so the screen cannot close it.
fn code_fence(screen: &str) -> String {
    let longest = screen
        .split(|ch| ch != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}
