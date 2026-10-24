//! A cloud session's status from GitHub: its branch's pushes and its pull request, read with `gh`
//! about once a minute, turned into the row's status line, a blue dot and a notification.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User picked status "from the branch and pull request": "pushed 3 commits", "PR opened", with a blue dot and a notification. Claude offers no way to read a cloud session's messages or state (no API, and attaching from a terminal is off for the account), so this is the status every client gets; the claude.ai page, while Ghostex's Browser shows it, adds working and needs-you on top (page_status.rs) and this poller is what remains when it cannot tell.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::{json, Map, Value};
use tokio::sync::Notify;

use super::github::{read_branch, status_line, BranchReport};
use super::record::{session_cloud_record, CloudState};
use super::start::{load, open, save};
use crate::domain::DomainRepository;
use crate::server::AppState;

/// One look at every running cloud session's branch.
const TICK: Duration = Duration::from_secs(60);
/// With no running cloud session the loop only waits for `wake_claude_cloud_poller`.
const IDLE_WAIT: Duration = Duration::from_secs(600);
/// A start older than this that is no longer running in this gxserver failed with it.
const STALE_START_SECS: i64 = 10 * 60;

fn wake() -> &'static Notify {
    static WAKE: OnceLock<Notify> = OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

/// Looks again now (a cloud session just started or was created).
pub(crate) fn wake_claude_cloud_poller() {
    wake().notify_one();
}

struct Watched {
    project_id: String,
    session_id: String,
    repo: String,
    branch: String,
}

/// The cloud sessions worth a look, settling starts that died with an earlier gxserver.
fn watched_sessions(state: &AppState) -> Vec<Watched> {
    let Ok(db) = open(state) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    // Only rows with a cloud record can pass `session_cloud_record`.
    let Ok(sessions) = repository.list_sessions_with_runtime_object(super::record::RECORD_KEY)
    else {
        return Vec::new();
    };
    let mut watched = Vec::new();
    for session in sessions {
        let Some(mut record) = session_cloud_record(&session) else {
            continue;
        };
        let text = |key: &str| session.get(key).and_then(Value::as_str).unwrap_or_default();
        let (project_id, session_id) =
            (text("projectId").to_string(), text("sessionId").to_string());
        if record.state == CloudState::Starting
            && !super::start::is_starting(&project_id, &session_id)
            && record
                .raw
                .get("startRequestedAt")
                .and_then(Value::as_str)
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .is_none_or(|at| chrono::Utc::now().timestamp() - at.timestamp() > STALE_START_SECS)
        {
            record.state = CloudState::Failed;
            record.error = Some(
                "Ghostex stopped while this session was starting. Start it again.".to_string(),
            );
            let _ = save(state, &db, &repository, &project_id, &session_id, &record);
            continue;
        }
        if record.state != CloudState::Running {
            continue;
        }
        if let (Some(repo), Some(branch)) = (record.repo.clone(), record.branch.clone()) {
            watched.push(Watched {
                project_id,
                session_id,
                repo,
                branch,
            });
        }
    }
    watched
}

/// What a report says, as one comparable string.
fn report_key(report: &BranchReport) -> String {
    let pull = report
        .pull_request
        .as_ref()
        .map(|(number, _, state)| format!("{number}:{state}"))
        .unwrap_or_default();
    format!(
        "{}|{}|{pull}",
        report.head.as_deref().unwrap_or_default(),
        report.ahead_by.unwrap_or_default()
    )
}

/// Applies one report; returns whether the user should hear about it.
fn apply_report(state: &AppState, watched: &Watched, report: &BranchReport) -> bool {
    let Some(line) = status_line(&watched.branch, report) else {
        return false;
    };
    let Ok(db) = open(state) else {
        return false;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok((_, mut record)) = load(&repository, &watched.project_id, &watched.session_id) else {
        return false;
    };
    if record.state != CloudState::Running || record.branch.as_deref() != Some(&watched.branch) {
        return false;
    }
    let key = report_key(report);
    if record.raw.get("githubSeen").and_then(Value::as_str) == Some(key.as_str()) {
        return false;
    }
    // A migrated or newly watched session takes what GitHub already shows without a notification.
    let quiet = record.raw.remove("quietFirstLook").is_some();
    record.raw.insert("githubSeen".to_string(), json!(key));
    record.status_text = Some(line);
    record.pr_url = report.pull_request.as_ref().map(|(_, url, _)| url.clone());
    save(
        state,
        &db,
        &repository,
        &watched.project_id,
        &watched.session_id,
        &record,
    )
    .is_ok()
        && !quiet
}

/// Puts the row in attention, the same path a local agent's needs-you takes (sidebar dot, the
/// notification feed and the desktop's banner); the feed's text is the row's status line
/// (`notification_text`).
pub(super) fn send_activity(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    activity: &str,
    turn_complete: bool,
) {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("agentName".to_string(), json!(super::CLAUDE_CLOUD_AGENT_ID));
    params.insert("activity".to_string(), json!(activity));
    if turn_complete {
        params.insert(
            "attentionSource".to_string(),
            json!(crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE),
        );
    }
    let _ = crate::server::dispatch_agent_http_blocking(
        state,
        "/api/updateAgentActivity".to_string(),
        "claude-cloud-status".to_string(),
        params,
    );
}

/// What the notification feed says for a cloud session: its latest status line.
pub(crate) fn notification_text(session: &Value) -> Option<String> {
    session_cloud_record(session)?.status_text
}

/// Starts the poller: a `gh` look at every running cloud session about once a minute, and nothing
/// while none runs.
pub(crate) fn start_claude_cloud_poller(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let migrating = state.clone();
        let _ = tokio::task::spawn_blocking(move || {
            super::work::migrate_work_cloud_records(&migrating)
        })
        .await;
        loop {
            let reader = state.clone();
            let watched = tokio::task::spawn_blocking(move || watched_sessions(&reader))
                .await
                .unwrap_or_default();
            let delay = if watched.is_empty() { IDLE_WAIT } else { TICK };
            for session in watched {
                let worker = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let Some(report) = read_branch(&session.repo, &session.branch) else {
                        return;
                    };
                    if apply_report(&worker, &session, &report) {
                        send_activity(
                            &worker,
                            &session.project_id,
                            &session.session_id,
                            "attention",
                            false,
                        );
                    }
                })
                .await;
            }
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(delay) => {}
                _ = wake().notified() => {}
            }
        }
    });
}
