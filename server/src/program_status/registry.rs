use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{current_status_source, StatusSource};

/// One OSC 7501 record as zmx/wmx send it (`{"reported":bool,"records":[...]}`).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgramStatusRecord {
    /// Empty for the root record.
    #[serde(default)]
    pub id: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

/// What gxserver knows about one session's program status.
#[derive(Clone, Debug, Default)]
pub(crate) struct SessionProgramStatus {
    /// The daemon answered the subscribe (it keeps OSC 7501 records).
    pub supported: bool,
    /// A program in the session reported and is still running (the daemon
    /// clears it once that program exits, so Auto goes back to detection).
    pub reported: bool,
    pub records: Vec<ProgramStatusRecord>,
    /// Identity of the record state last written to the session's activity,
    /// so a re-sent snapshot (another record changed) never re-raises a
    /// finished turn the user already acknowledged.
    pub applied_key: Option<String>,
    /// The derived state behind `applied_key` (idle, working, ...), so a
    /// working → idle step can be told apart from a session that was idle.
    pub applied_state: Option<String>,
    /// The user interrupted the current turn from Ghostex (Escape in the
    /// terminal pane or Stop in the chat) since the session last went working.
    pub interrupted: bool,
    /// The daemon's count of submitted input (an Enter reaching the session
    /// from a terminal, a chat send, `ghostex send-text` and the like); `None`
    /// from a daemon that predates it.
    pub submits: Option<u64>,
    /// `submits` when the last working stint began (or when the program was
    /// first seen), so each stint can tell whether input arrived since.
    pub submits_at_working_start: Option<u64>,
    /// Input was submitted between the previous working stint's start and this
    /// one's: it is a turn (a prompt typed ahead during the last turn counts).
    /// A program's own start-up (idle, working while it loads, idle) has none.
    pub working_after_input: bool,
}

fn registry() -> &'static Mutex<HashMap<String, SessionProgramStatus>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, SessionProgramStatus>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn registry_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}\u{0}{session_id}")
}

pub(crate) fn program_status_entry(
    project_id: &str,
    session_id: &str,
) -> Option<SessionProgramStatus> {
    registry()
        .lock()
        .ok()?
        .get(&registry_key(project_id, session_id))
        .cloned()
}

/// Store a snapshot; returns true when the records changed.
pub(crate) fn store_program_status_snapshot(
    project_id: &str,
    session_id: &str,
    reported: bool,
    submits: Option<u64>,
    records: Vec<ProgramStatusRecord>,
) -> bool {
    let Ok(mut registry) = registry().lock() else {
        return false;
    };
    let entry = registry
        .entry(registry_key(project_id, session_id))
        .or_default();
    let changed = !entry.supported || entry.reported != reported || entry.records != records;
    entry.supported = true;
    entry.reported = reported;
    entry.submits = submits;
    entry.records = records;
    changed
}

/// The daemon predates OSC 7501 support: the session stays on detection.
pub(crate) fn mark_program_status_unsupported(project_id: &str, session_id: &str) {
    if let Ok(mut registry) = registry().lock() {
        registry.insert(
            registry_key(project_id, session_id),
            SessionProgramStatus::default(),
        );
    }
}

pub(crate) fn set_program_status_applied_key(
    project_id: &str,
    session_id: &str,
    key: Option<String>,
) {
    set_program_status_applied(project_id, session_id, key, None);
}

/// Record what was last written to the session's activity. Entering working
/// starts a stint: it forgets an earlier interrupt and notes whether input
/// was submitted since the previous stint began.
pub(crate) fn set_program_status_applied(
    project_id: &str,
    session_id: &str,
    key: Option<String>,
    state: Option<&str>,
) {
    if let Ok(mut registry) = registry().lock() {
        if let Some(entry) = registry.get_mut(&registry_key(project_id, session_id)) {
            if entry.submits_at_working_start.is_none() {
                entry.submits_at_working_start = entry.submits;
            }
            if state == Some("working") && entry.applied_state.as_deref() != Some("working") {
                entry.interrupted = false;
                entry.working_after_input = matches!(
                    (entry.submits, entry.submits_at_working_start),
                    (Some(now), Some(before)) if now > before
                );
                entry.submits_at_working_start = entry.submits;
            }
            entry.applied_key = key;
            entry.applied_state = state.map(str::to_string);
        }
    }
}

/// The user pressed Escape in the terminal pane or Stop in the chat.
pub(crate) fn note_program_status_interrupt(project_id: &str, session_id: &str) {
    if let Ok(mut registry) = registry().lock() {
        if let Some(entry) = registry.get_mut(&registry_key(project_id, session_id)) {
            entry.interrupted = true;
        }
    }
}

/// Forget a session whose watch ended (session stopped or slept): its
/// daemon, and with it every record, is gone.
pub(crate) fn forget_program_status(project_id: &str, session_id: &str) -> bool {
    registry()
        .lock()
        .ok()
        .and_then(|mut registry| registry.remove(&registry_key(project_id, session_id)))
        .is_some()
}

/// Every session gxserver holds program status for, as (project, session).
pub(crate) fn program_status_sessions() -> Vec<(String, String)> {
    let Ok(registry) = registry().lock() else {
        return Vec::new();
    };
    registry
        .keys()
        .filter_map(|key| key.split_once('\u{0}'))
        .map(|(project, session)| (project.to_string(), session.to_string()))
        .collect()
}

/// Whether the session's activity comes from its OSC 7501 records instead of
/// Ghostex's own detection, under `source`.
pub(crate) fn program_status_governs_entry(
    entry: Option<&SessionProgramStatus>,
    source: StatusSource,
) -> bool {
    let Some(entry) = entry else {
        return false;
    };
    match source {
        StatusSource::Detection => false,
        StatusSource::Auto => entry.supported && entry.reported,
        StatusSource::ProgramStatus => entry.supported,
    }
}

pub(crate) fn program_status_governs(project_id: &str, session_id: &str) -> bool {
    let source = current_status_source();
    if source == StatusSource::Detection {
        return false;
    }
    program_status_governs_entry(
        program_status_entry(project_id, session_id).as_ref(),
        source,
    )
}

/// `program_status_governs` for a stored session row.
pub(crate) fn program_status_governs_session(session: &Value) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::trim);
    match (text("projectId"), text("sessionId")) {
        (Some(project_id), Some(session_id))
            if !project_id.is_empty() && !session_id.is_empty() =>
        {
            program_status_governs(project_id, session_id)
        }
        _ => false,
    }
}
