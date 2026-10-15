use serde_json::{json, Value};

use crate::logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel};

use super::{
    program_status_logger, take_settled_program_status_disagreements,
    track_program_status_agreement,
};

/// How long detection and the program may disagree before it counts as a
/// mismatch: a program reporting a change before detection notices it is the
/// improvement Combined is for, not a mismatch.
const SETTLE_WINDOW_MS: i64 = 10_000;

/// The activity a program state stands for (`program_status_activity` without
/// the attention source).
pub(crate) fn program_state_activity(program_state: &str) -> &'static str {
    match program_state {
        "working" => "working",
        "blocked" | "done" | "error" | "finished" => "attention",
        _ => "idle",
    }
}

fn session_text<'a>(session: &'a Value, key: &str) -> Option<&'a str> {
    session
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Detection computed `detected` for a session under Combined, from `input`.
pub(crate) fn observe_detection_opinion(session: &Value, detected: &str, input: Value) {
    let (Some(project_id), Some(session_id)) = (
        session_text(session, "projectId"),
        session_text(session, "sessionId"),
    ) else {
        return;
    };
    track_program_status_agreement(
        project_id,
        session_id,
        Some(detected),
        session_text(session, "agentId"),
        Some(input),
    );
}

/// The program's state changed for a session under Combined.
pub(crate) fn observe_program_opinion(project_id: &str, session_id: &str) {
    track_program_status_agreement(project_id, session_id, None, None, None);
}

/// CDXC:SessionStatus 2026-10-15 DECISION:
/// User (Q121): "yes to a mismatch log". Under Combined, a disagreement between Ghostex's detection and a program's OSC 7501 report that is still there after the 10-second settle window goes to the Program status mismatches diagnostic log once (session, agent, both states, which one the session shows, and what detection last saw: the input kind, hook event and terminal title), so each real mismatch can become a detection fix; the program simply being faster than detection is not logged (the regression test found every program-led step logged). It writes only while Show debug UI controls and that area are on (`log_routine`). Its detail keys avoid the logger's always-persist markers (`error`, `status`, ...), so a program state of `error` cannot make a line persist with the area off.
/// SEE-ALSO: packages/settings-catalog/src/data/diagnostic_logging.rs (`gxserver.programStatusMismatch`), server/src/session_status/model.rs (detection's opinion), server/src/program_status/ingest.rs (the program's opinion and the periodic check).
pub(crate) fn log_settled_program_status_mismatches() {
    let settled = take_settled_program_status_disagreements(SETTLE_WINDOW_MS);
    if settled.is_empty() {
        return;
    }
    let Some((logger, server_id)) = program_status_logger() else {
        return;
    };
    for (project_id, session_id, pending) in settled {
        let program_wins = (pending.program_state == "blocked" && pending.detected != "attention")
            || (pending.program_state == "working" && pending.detected == "idle");
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
                    "agent": pending.agent,
                    "detectedActivity": pending.detected,
                    "programState": pending.program_state,
                    "winner": if program_wins { "program" } else { "detection" },
                    "settleWindowMs": SETTLE_WINDOW_MS,
                    "detectionInput": pending.input,
                })),
            },
        );
    }
}
