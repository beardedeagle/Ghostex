//! What must survive the restart an update causes: the switch, the install in flight and how the last one ended.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::constants::GXSERVER_VERSION;
use crate::paths::GxserverPaths;

/// How long an install may take, from the request to the new gxserver answering, before it counts as failed.
const PENDING_INSTALL_TIMEOUT_MS: i64 = 15 * 60 * 1000;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AppUpdateRecord {
    #[serde(default)]
    pub(super) auto_update: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) pending_install: Option<PendingInstall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) last_result: Option<InstallResult>,
    /// The version the switch last tried, so a failing update is not retried every pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) last_auto_attempt: Option<AutoAttempt>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PendingInstall {
    pub(super) from_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) target_version: Option<String>,
    pub(super) requested_at: String,
    /// `request` (a client's Update button or the CLI) or `auto` (the keep-up-to-date switch).
    pub(super) source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallResult {
    /// `installed` or `failed`.
    pub(super) outcome: String,
    pub(super) from_version: String,
    pub(super) to_version: String,
    pub(super) finished_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AutoAttempt {
    pub(super) version: String,
    pub(super) at: String,
}

fn record_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join("app-update.json")
}

pub(super) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(super) fn age_ms(iso: &str) -> Option<i64> {
    let at = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    Some(chrono::Utc::now().timestamp_millis() - at.timestamp_millis())
}

pub(super) fn read_record(paths: &GxserverPaths) -> AppUpdateRecord {
    std::fs::read_to_string(record_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(super) fn write_record(paths: &GxserverPaths, record: &AppUpdateRecord) {
    let path = record_path(paths);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(text) = serde_json::to_string_pretty(record) else {
        return;
    };
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, text).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

/// The record with a finished install moved into `last_result`: a gxserver running another version than the one the install started from is the new build answering; the same version after the timeout means the install never landed.
pub(super) fn read_settled_record(paths: &GxserverPaths) -> AppUpdateRecord {
    let mut record = read_record(paths);
    let Some(pending) = record.pending_install.clone() else {
        return record;
    };
    let settled = if pending.from_version != GXSERVER_VERSION {
        Some(InstallResult {
            outcome: "installed".to_string(),
            from_version: pending.from_version,
            to_version: GXSERVER_VERSION.to_string(),
            finished_at: now_iso(),
            message: None,
        })
    } else if age_ms(&pending.requested_at).is_none_or(|age| age > PENDING_INSTALL_TIMEOUT_MS) {
        Some(InstallResult {
            outcome: "failed".to_string(),
            from_version: pending.from_version.clone(),
            to_version: pending.from_version,
            finished_at: now_iso(),
            message: Some(
                "The update did not finish; Ghostex is still on the same version.".to_string(),
            ),
        })
    } else {
        None
    };
    if let Some(result) = settled {
        record.pending_install = None;
        record.last_result = Some(result);
        write_record(paths, &record);
    }
    record
}
