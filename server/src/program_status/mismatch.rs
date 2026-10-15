use serde_json::{json, Value};

use crate::logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel};

use super::{note_program_status_mismatch, program_status_logger};

/// The activity a program state stands for (`program_status_activity` without
/// the attention source).
pub(crate) fn program_state_activity(program_state: &str) -> &'static str {
    match program_state {
        "working" => "working",
        "blocked" | "done" | "error" | "finished" => "attention",
        _ => "idle",
    }
}

/// CDXC:SessionStatus 2026-10-15 DECISION:
/// User (Q121): "yes to a mismatch log". Under Combined, each time Ghostex's detection and a program's OSC 7501 report disagree about a session, one line goes to the Program status mismatches diagnostic log (session, agent, both states, which won, and what detection saw: the input kind, hook event and terminal title), so each mismatch can become a detection fix. A disagreement that lasts is logged once. It writes only while Show debug UI controls and that area are on (`log_routine`). Its detail keys avoid the logger's always-persist markers (`error`, `status`, ...), so a program state of `error` cannot make a line persist with the area off.
/// SEE-ALSO: packages/settings-catalog/src/data/diagnostic_logging.rs (`gxserver.programStatusMismatch`).
pub(crate) fn log_program_status_mismatch(
    session: &Value,
    detected_activity: &str,
    program_state: &str,
    winner: &str,
    detection_input: Value,
) {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return;
    };
    let key = format!("{detected_activity}|{program_state}|{winner}");
    if !note_program_status_mismatch(project_id, session_id, &key) {
        return;
    }
    let Some((logger, server_id)) = program_status_logger() else {
        return;
    };
    let _ = logger.log_routine(
        DiagnosticLogScenario::ProgramStatusMismatch,
        GxserverLogInput {
            level: LogLevel::Debug,
            event: "programStatus.mismatch".to_string(),
            server_id: Some(server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "agent": text("agentId"),
                "detectedActivity": detected_activity,
                "programState": program_state,
                "winner": winner,
                "detectionInput": detection_input,
            })),
        },
    );
}
