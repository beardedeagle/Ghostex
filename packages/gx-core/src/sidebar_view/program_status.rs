//! The hover line a session row shows for its program's OSC 7501 report.

use ghostex_gx_protocol::ProgramStatus;

/// CDXC:SessionStatus 2026-10-14 DECISION:
/// User (Q114): the program's one-line `msg` shows in the sidebar only on hover, as a line of the session row's tooltip, never as always-visible text. The line says what the program is waiting for (permission, an answer, a sign-in) or how far it got, then its own words; gxserver already stripped control and direction-override characters, and the tooltip draws it as plain text.
/// SEE-ALSO: server/src/program_status/presentation.rs (`message`), apps/desktop/src/app/native_sidebar/sessions.rs (the tooltip).
pub(crate) fn program_status_hover_line(status: &ProgramStatus) -> Option<String> {
    let message = status
        .message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty());
    let label = match status.state.as_str() {
        "blocked" => Some(match status.kind.as_deref() {
            Some("permission") => "Needs permission".to_string(),
            Some("question") => "Needs an answer".to_string(),
            Some("auth") => "Needs sign-in".to_string(),
            _ => "Waiting for you".to_string(),
        }),
        "working" => status.progress.map(|progress| format!("{progress}%")),
        "error" => Some("Failed".to_string()),
        _ => None,
    };
    match (label, message) {
        (Some(label), Some(message)) => Some(format!("{label} · {message}")),
        (Some(label), None) => Some(label),
        (None, Some(message)) => Some(message.to_string()),
        (None, None) => None,
    }
}
