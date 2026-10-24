//! The offer a work-mode session gets once its linked PR is merged: remove its worktree and park
//! it, or keep it. The answer is stored on the session as `runtimeSettings.workCleanup`, keyed on
//! the PR, so the offer never comes back for that PR on any client.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: when a linked PR is merged, Ghostex offers to remove the worktree and park the session, not just put it to sleep. The offer is shown once per PR: after "Clean up" or "Keep" it does not come back for the same PR. Since 2026-10-24 the **Offer Clean up when a PR merges** setting (on by default) can turn it off (CDXC:WorkMode 2026-10-24 in packages/settings-catalog/src/data/defaults.rs).
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! "Already offered" lives on the session in gxserver, not in a client, so the desktop, the web
//! build and the phone agree and an answer given on one is gone from the others.
//!
//! SEE-ALSO: `/api/answerWorkCleanup` in server/src/server/route_http/work_links.rs (the answer),
//! apps/desktop/src/app/native_sidebar/work_chips.rs (the card's Clean up / Keep chips).

use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::domain::DomainStateError;
use crate::worktree_sessions::read_worktree_session_marker;

/// What identifies a PR in the stored answer: its URL, else `#<number>`.
pub(crate) fn work_cleanup_pull_request_key(number: u64, url: Option<&str>) -> String {
    url.map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{number}"))
}

fn answered_pull_request(session: &Value) -> Option<&str> {
    session
        .pointer("/runtimeSettings/workCleanup/pullRequest")
        .and_then(Value::as_str)
}

/// The worktree folder gxserver made for this session, when it is still on disk.
pub(crate) fn work_cleanup_worktree_path(session: &Value) -> Option<String> {
    read_worktree_session_marker(session)
        .map(|marker| marker.path)
        .filter(|path| Path::new(path).is_dir())
}

/// Whether to offer the cleanup for this merged PR: not answered for it yet, and something is
/// left to do (the session is not parked yet, or its worktree is still there).
pub(crate) fn offers_work_cleanup(session: &Value, pull_request_key: &str) -> bool {
    if answered_pull_request(session) == Some(pull_request_key) {
        return false;
    }
    let parked = session.get("isParked").and_then(Value::as_bool) == Some(true);
    !parked || work_cleanup_worktree_path(session).is_some()
}

/// Records the answer for this PR, patched in place like the links (links.rs).
pub(crate) fn write_work_cleanup_answer(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    pull_request_key: &str,
    answer: &str,
    answered_at: &str,
) -> Result<(), DomainStateError> {
    let record = json!({
        "pullRequest": pull_request_key,
        "answer": answer,
        "answeredAt": answered_at,
    });
    db.execute(
        "UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{}'), '$.workCleanup', json(?1)) \
         WHERE projectId = ?2 AND sessionId = ?3",
        params![record.to_string(), project_id, session_id],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite work cleanup error: {error}"),
    })?;
    Ok(())
}
