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
    /// The last disagreement written to the mismatch log (detection, program).
    pub last_mismatch: Option<String>,
    /// Under Combined: the activity Ghostex's detection last computed for the
    /// session on its own (before the Combined guard).
    pub detection_activity: Option<String>,
    /// Under Combined: detection and the program disagree, since when, and
    /// what detection last saw. Logged only once it outlasts the settle window.
    pub disagreement: Option<Disagreement>,
    /// The session's agent id (or `agent` for an agent session without one);
    /// `None` for a plain shell or script, which detection is not meant to
    /// detect, so its disagreements are never logged.
    pub agent: Option<String>,
    /// While the program drives the session: Ghostex detection's own view of
    /// it (an `agentActivity` object), moved by every detection input, and
    /// what the session shows again when the program stops driving it.
    pub detection_shadow: Option<Value>,
    /// The shadow started from detection's own view (the program appeared
    /// after a snapshot with no records). After a gxserver restart the first
    /// snapshot already has records, so the shadow can only start from what
    /// the program wrote and is not trusted to say idle or working.
    pub shadow_known: bool,
    /// A snapshot with no records was seen for this daemon.
    pub saw_empty_records: bool,
    /// The report (its applied key) whose attention was last announced, and
    /// that attention's event id. Kept across hand-backs and setting changes,
    /// so applying the same report again reuses the event id instead of
    /// notifying the user a second time.
    pub notified: Option<(String, String)>,
}

pub(crate) fn set_program_status_notified(
    project_id: &str,
    session_id: &str,
    key: String,
    event_id: String,
) {
    if let Ok(mut registry) = registry().lock() {
        if let Some(entry) = registry.get_mut(&registry_key(project_id, session_id)) {
            entry.notified = Some((key, event_id));
        }
    }
}

fn session_ids(session: &Value) -> Option<(String, String)> {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some((text("projectId")?, text("sessionId")?))
}

/// Detection's shadow view of a stored session row, when one is kept.
pub(crate) fn program_status_detection_shadow(session: &Value) -> Option<Value> {
    let (project_id, session_id) = session_ids(session)?;
    program_status_entry(&project_id, &session_id)?.detection_shadow
}

pub(crate) fn set_program_status_detection_shadow(session: &Value, shadow: Value) {
    let Some((project_id, session_id)) = session_ids(session) else {
        return;
    };
    if let Ok(mut registry) = registry().lock() {
        if let Some(entry) = registry.get_mut(&registry_key(&project_id, &session_id)) {
            entry.detection_shadow = Some(shadow);
        }
    }
}

/// Start detection's shadow from the session's current activity when the
/// program begins to drive it (detection's view up to that moment).
pub(crate) fn start_program_status_detection_shadow(
    project_id: &str,
    session_id: &str,
    current: Option<&Value>,
) {
    if let Ok(mut registry) = registry().lock() {
        if let Some(entry) = registry.get_mut(&registry_key(project_id, session_id)) {
            if entry.detection_shadow.is_none() {
                entry.detection_shadow = Some(
                    current
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({"activity": "idle"})),
                );
                entry.shadow_known = entry.saw_empty_records;
            }
        }
    }
}

/// Take detection's shadow (it stops being kept) when the program stops
/// driving the session.
pub(crate) fn take_program_status_detection_shadow(
    project_id: &str,
    session_id: &str,
) -> Option<Value> {
    let mut registry = registry().lock().ok()?;
    registry
        .get_mut(&registry_key(project_id, session_id))?
        .detection_shadow
        .take()
}

#[derive(Clone, Debug)]
pub(crate) struct Disagreement {
    pub since_ms: i64,
    pub detected: String,
    pub program_state: String,
    pub agent: Option<String>,
    pub input: Value,
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Compare detection's last opinion with the program's state for a session
/// and start, keep or end its pending disagreement. `detected` updates
/// detection's opinion when given; `input` is what detection last saw.
pub(crate) fn track_program_status_agreement(
    project_id: &str,
    session_id: &str,
    detected: Option<&str>,
    agent: Option<&str>,
    input: Option<Value>,
) {
    let Ok(mut registry) = registry().lock() else {
        return;
    };
    let Some(entry) = registry.get_mut(&registry_key(project_id, session_id)) else {
        return;
    };
    if let Some(detected) = detected {
        entry.detection_activity = Some(detected.to_string());
    }
    if let Some(agent) = agent {
        entry.agent = Some(agent.to_string());
    }
    let program_state = entry.applied_state.clone().unwrap_or_else(|| {
        super::derive_program_status(&entry.records)
            .state
            .to_string()
    });
    let detected = entry
        .detection_activity
        .clone()
        .unwrap_or_else(|| "idle".to_string());
    if detected == super::program_state_activity(&program_state) {
        entry.disagreement = None;
        entry.last_mismatch = None;
        return;
    }
    let same = entry.disagreement.as_ref().is_some_and(|pending| {
        pending.detected == detected && pending.program_state == program_state
    });
    if same {
        if let (Some(pending), Some(input)) = (entry.disagreement.as_mut(), input) {
            pending.input = input;
        }
        return;
    }
    entry.disagreement = Some(Disagreement {
        since_ms: unix_now_ms(),
        detected,
        program_state,
        agent: entry.agent.clone(),
        input: input.unwrap_or(Value::Null),
    });
}

/// Disagreements that outlasted `window_ms` and were not logged yet, as
/// (project, session, disagreement); each is returned once.
pub(crate) fn take_settled_program_status_disagreements(
    window_ms: i64,
) -> Vec<(String, String, Disagreement)> {
    let Ok(mut registry) = registry().lock() else {
        return Vec::new();
    };
    let now = unix_now_ms();
    let source = current_status_source();
    let mut due = Vec::new();
    for (key, entry) in registry.iter_mut() {
        // A session that stopped combining (its program exited, or the
        // setting changed) has nothing left to disagree about.
        if !program_status_combines_entry(Some(entry), source) || entry.agent.is_none() {
            entry.disagreement = None;
            continue;
        }
        let Some(pending) = entry.disagreement.as_ref() else {
            continue;
        };
        if now - pending.since_ms < window_ms {
            continue;
        }
        let mark = format!("{}|{}", pending.detected, pending.program_state);
        if entry.last_mismatch.as_deref() == Some(mark.as_str()) {
            continue;
        }
        entry.last_mismatch = Some(mark);
        if let Some((project, session)) = key.split_once('\u{0}') {
            due.push((project.to_string(), session.to_string(), pending.clone()));
        }
    }
    due
}

/// Forget the applied report's identity but keep the state the program
/// raised, so re-applying after a setting change can still hand a working or
/// blocked state back to detection.
pub(crate) fn reset_program_status_applied_key(project_id: &str, session_id: &str) {
    if let Ok(mut registry) = registry().lock() {
        if let Some(entry) = registry.get_mut(&registry_key(project_id, session_id)) {
            entry.applied_key = None;
        }
    }
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
    if records.is_empty() {
        entry.saw_empty_records = true;
    }
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
        StatusSource::Detection | StatusSource::Combined => false,
        StatusSource::ProgramStatus => entry.supported,
    }
}

/// Whether the session's reports join Ghostex's detection under Combined: its
/// daemon keeps records and the program that reported is still running.
pub(crate) fn program_status_combines_entry(
    entry: Option<&SessionProgramStatus>,
    source: StatusSource,
) -> bool {
    source == StatusSource::Combined && entry.is_some_and(|entry| entry.supported && entry.reported)
}

/// Under Combined, the program's derived state for a stored session row, or
/// `None` when its reports do not join detection.
pub(crate) fn program_status_combined_state(session: &Value) -> Option<&'static str> {
    let source = current_status_source();
    if source != StatusSource::Combined {
        return None;
    }
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::trim);
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return None;
    };
    let entry = program_status_entry(project_id, session_id)?;
    program_status_combines_entry(Some(&entry), source)
        .then(|| super::derive_program_status(&entry.records).state)
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
