//! A cloud session's live status from its claude.ai page: the desktop reads the page Ghostex's
//! Browser shows (apps/desktop/src/app/claude_cloud_page/) and reports it here as `pageStatus`.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User: "also we need to think to how to detect working vs attention from the html page itself". While the session's claude.ai page is open in Ghostex's Browser, what it shows drives the row: working (orange), needs you (blue), and a turn that ended (Done). Anything the page reader cannot recognise is `unknown` and changes nothing, so the branch and pull request status (poller.rs) is what remains.

use serde_json::{json, Map, Value};

use super::poller::send_activity;
use super::record::{normalized_session_url, session_cloud_record, CloudState, RECORD_KEY};
use super::start::{open, save};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

/// `pageStatus {url, status, branch?, prUrl?}`: `status` is `working`, `needsInput`, `idle` or
/// `unknown`.
pub(crate) fn apply_page_status(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let url = params
        .get("url")
        .and_then(Value::as_str)
        .and_then(normalized_session_url)
        .ok_or_else(|| {
            DomainStateError::bad_request("url must be a claude.ai/code session link.")
        })?;
    let status = params
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if !matches!(status, "working" | "needsInput" | "idle" | "unknown") {
        return Err(DomainStateError::bad_request(
            "status must be working, needsInput, idle or unknown.",
        ));
    }
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    // Only a row whose `claudeCloud` is an object carries a cloud record, so SQLite picks those
    // rows (in the same order) instead of every session row being hydrated for each report.
    let Some((session, mut record)) = repository
        .list_sessions_with_runtime_object(RECORD_KEY)?
        .into_iter()
        .find_map(|session| {
            let record = session_cloud_record(&session)?;
            (record.state == CloudState::Running && record.url.as_deref() == Some(url.as_str()))
                .then_some((session, record))
        })
    else {
        // A claude.ai page that is not one of Ghostex's cloud sessions.
        return Ok(json!({ "ok": true, "matched": false }));
    };
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let mut changed = false;
    // The page names the branch the session really pushes to; Claude may pick its own name.
    if let Some(branch) = params
        .get("branch")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|branch| is_branch_name(branch))
    {
        if record.branch.as_deref() != Some(branch) {
            record.branch = Some(branch.to_string());
            record.raw.remove("githubSeen");
            changed = true;
        }
    }
    if let Some(pr_url) = params
        .get("prUrl")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("https://github.com/") && url.contains("/pull/"))
    {
        if record.pr_url.as_deref() != Some(pr_url) {
            record.pr_url = Some(pr_url.to_string());
            changed = true;
        }
    }
    // The page says the account is out of usage: the row's notice names the full window and its
    // reset time; it goes once the page shows the session working again.
    let limit_seen = params
        .get("limit")
        .and_then(Value::as_str)
        .is_some_and(|limit| !limit.trim().is_empty());
    let mut limit_entered = false;
    if limit_seen && status != "working" {
        let notice =
            super::accounts::limit_notice(&db, record.raw.get("accountId").and_then(Value::as_str));
        if record.raw.get("limitNotice").and_then(Value::as_str) != Some(notice.as_str()) {
            limit_entered = !record.raw.contains_key("limitNotice");
            record.raw.insert("limitNotice".to_string(), json!(notice));
            changed = true;
        }
    } else if status == "working" && record.raw.remove("limitNotice").is_some() {
        changed = true;
    }
    let previous = record
        .raw
        .get("pageStatus")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    if status != "unknown" && status != previous {
        record.raw.insert("pageStatus".to_string(), json!(status));
        changed = true;
    }
    if changed {
        save(state, &db, &repository, &project_id, &session_id, &record)?;
    }
    drop(repository);
    drop(db);
    if limit_entered {
        send_activity(state, &project_id, &session_id, "attention", false);
    }
    if status != previous {
        match status {
            "working" => send_activity(state, &project_id, &session_id, "working", false),
            "needsInput" => send_activity(state, &project_id, &session_id, "attention", false),
            // A turn that ended on the page: Done, like a local agent's Stop hook.
            "idle" if previous == "working" => {
                send_activity(state, &project_id, &session_id, "attention", true)
            }
            "idle" if previous == "needsInput" => {
                send_activity(state, &project_id, &session_id, "idle", false)
            }
            _ => {}
        }
    }
    Ok(json!({ "ok": true, "matched": true }))
}

/// A Git branch name as the page shows it (`claude/fix-the-thing-a1b2`), nothing else.
fn is_branch_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with(['-', '/'])
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._/-".contains(&byte))
}
