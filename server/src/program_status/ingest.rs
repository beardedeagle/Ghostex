use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::agents::dispatch_agent_endpoint;
use crate::domain::DomainRepository;
use crate::server::{
    log_agent_activity_transition, schedule_presentation_session_delta,
    schedule_stale_activity_presentation_refresh, AppState,
};
use crate::session_status::parse_iso_ms;
use crate::storage::open_gxserver_database;

use super::{
    current_status_source, derive_program_status, forget_program_status,
    mark_program_status_unsupported, program_status_activity, program_status_applied_key,
    program_status_entry, program_status_governs_entry, program_status_sessions,
    set_program_status_applied_key, store_program_status_snapshot, ProgramStatusRecord,
};

/// One program-status line of `zmx watch-title` / `wmx watch-title`.
#[derive(Debug, PartialEq)]
pub(crate) enum ProgramStatusLine {
    Snapshot {
        reported: bool,
        records: Vec<ProgramStatusRecord>,
    },
    /// The daemon predates OSC 7501 support.
    Unsupported,
}

#[derive(Deserialize)]
struct SnapshotWire {
    #[serde(default)]
    reported: bool,
    #[serde(default)]
    records: Vec<ProgramStatusRecord>,
}

pub(crate) fn parse_program_status_line(line: &str) -> Option<ProgramStatusLine> {
    if !line.trim_start().starts_with("{\"programStatus") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("programStatusSupported") == Some(&Value::Bool(false)) {
        return Some(ProgramStatusLine::Unsupported);
    }
    let snapshot: SnapshotWire =
        serde_json::from_value(value.get("programStatus")?.clone()).ok()?;
    Some(ProgramStatusLine::Snapshot {
        reported: snapshot.reported,
        records: snapshot.records,
    })
}

/// Record a watch line and bring the session's activity in line with it.
pub(crate) fn ingest_program_status_line(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    line: ProgramStatusLine,
) {
    let records_changed = match line {
        ProgramStatusLine::Unsupported => {
            mark_program_status_unsupported(project_id, session_id);
            false
        }
        ProgramStatusLine::Snapshot { reported, records } => {
            store_program_status_snapshot(project_id, session_id, reported, records)
        }
    };
    apply_program_status(state, project_id, session_id, records_changed);
}

/// The watch of a session ended: its daemon and records are gone.
pub(crate) fn end_program_status_watch(state: &AppState, project_id: &str, session_id: &str) {
    let governed = program_status_governs_entry(
        program_status_entry(project_id, session_id).as_ref(),
        current_status_source(),
    );
    if forget_program_status(project_id, session_id) && governed {
        publish_session(state, project_id, session_id);
    }
}

fn publish_session(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
}

/// CDXC:SessionStatus 2026-10-14 WHY:
/// A program's report is written into the session's stored activity (not only shown) so everything that already hangs off activity keeps working from it: notifications and sounds on attention, the chat queue delivering after a finished turn, auto-sleep, recency and acknowledgement. The write goes through `/api/updateAgentActivity` with `programStatusActivity`, the one input the detection gate in `compute_activity_update` lets through for a governed session.
pub(crate) fn apply_program_status(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    records_changed: bool,
) {
    let Some(entry) = program_status_entry(project_id, session_id) else {
        return;
    };
    let governs = program_status_governs_entry(Some(&entry), current_status_source());
    if !governs {
        if entry.applied_key.is_some() {
            set_program_status_applied_key(project_id, session_id, None);
        }
        if records_changed {
            publish_session(state, project_id, session_id);
        }
        return;
    }
    let derived = derive_program_status(&entry.records);
    let key = program_status_applied_key(&derived);
    if entry.applied_key.as_deref() == Some(key.as_str()) {
        if records_changed {
            publish_session(state, project_id, session_id);
        }
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    set_program_status_applied_key(project_id, session_id, Some(key));
    if entry.applied_key.is_none() && already_seen(&session, derived.state, derived.record.as_ref())
    {
        if records_changed {
            let _ = schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                project_id,
                session_id,
            );
        }
        return;
    }
    let (activity, attention_source) = program_status_activity(derived.state);
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("programStatusActivity".to_string(), json!(activity));
    if let Some(source) = attention_source {
        params.insert("attentionSource".to_string(), json!(source));
    }
    let endpoint = "/api/updateAgentActivity";
    let Ok(output) = dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        endpoint,
        &params,
        None,
    ) else {
        return;
    };
    log_agent_activity_transition(state, endpoint, &params, &output.result);
    crate::notification_feed::observe_agent_activity_result(
        state,
        endpoint,
        &params,
        &output.result,
    );
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
    if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
        schedule_stale_activity_presentation_refresh(state, &session, "program-status");
    }
}

/// A finished or failed record that was already there when gxserver first
/// saw it (after a gxserver restart, or the setting turned on) and that the
/// user acknowledged after it was reported is not news: do not raise it again.
fn already_seen(session: &Value, state: &str, record: Option<&ProgramStatusRecord>) -> bool {
    if !matches!(state, "done" | "error") {
        return false;
    }
    let Some(record) = record else {
        return false;
    };
    let activity = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("agentActivity"));
    let is = |key: &str| activity.and_then(|value| value.get(key));
    is("activity").and_then(Value::as_str) == Some("idle")
        && is("isAcknowledged").and_then(Value::as_bool) == Some(true)
        && is("lastChangedAt")
            .and_then(Value::as_str)
            .and_then(parse_iso_ms)
            .is_some_and(|acknowledged_at| acknowledged_at >= record.updated_at_ms)
}

/// Re-apply every known session when the Session status setting changes, so
/// switching it takes effect at once instead of at each session's next report.
pub(crate) async fn run_status_source_watch(state: AppState) {
    let mut last = current_status_source();
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let source = current_status_source();
        if source == last {
            continue;
        }
        last = source;
        let state = state.clone();
        let _ = tokio::task::spawn_blocking(move || {
            for (project_id, session_id) in program_status_sessions() {
                set_program_status_applied_key(&project_id, &session_id, None);
                apply_program_status(&state, &project_id, &session_id, true);
            }
        })
        .await;
    }
}
