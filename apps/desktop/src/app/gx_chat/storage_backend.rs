//! Where the chat's client-storage rows live on the desktop: the one client-storage database.
//!
//! A catalog row on `local` lives in the `preferences` table as `(key, value)` with the raw string;
//! a row on `indexeddb` lives in `records` as `(key, store, value)` where `value` is the whole row
//! JSON. Both go through the connection pool `src/app/gx_store/sidebar_ui_storage.rs` owns, for the
//! reason written down there: a second pool would hold the write lock against the client-storage
//! service. The GPUI web build has its own file of this name, over the page's
//! `packages/client-storage`.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ghostex_client_storage::RecordWrite;

use crate::app::gx_store::{
    RecordRead, RecordStore, read_record_raw, remove_record, scan_record_raw, write_record,
};

use super::storage::{Backend, RecordBounds};

/// How often one store's refused or failed writes are written to the log while they keep coming.
const FAILED_WRITE_LOG_INTERVAL: Duration = Duration::from_secs(60);
/// How many distinct `(store, reason)` pairs the log throttle remembers.
const MAX_FAILED_WRITE_KINDS: usize = 32;

fn record_store(bounds: RecordBounds) -> RecordStore {
    RecordStore {
        id: bounds.id,
        version: bounds.version,
        max_entry_bytes: bounds.max_entry_bytes,
        max_bytes: bounds.max_bytes,
        max_entries: bounds.max_entries,
        max_age_ms: bounds.max_age_ms,
    }
}

/// One row's raw value, `None` when it is absent or expired.
pub(super) fn read(
    _store_id: &str,
    backend: Backend,
    name: &str,
    now_ms: i64,
) -> Result<Option<String>, &'static str> {
    match backend {
        Backend::Local => crate::app::gx_store::with_read_connection(|connection| {
            connection
                .query_row(
                    "SELECT value FROM preferences WHERE key=?1",
                    [name],
                    |row| row.get::<_, String>(0),
                )
                .map(Some)
                .or_else(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    _ => Err("query"),
                })
        }),
        Backend::Records(bounds) => match read_record_raw(record_store(bounds), name, now_ms)? {
            RecordRead::Payload(raw) => Ok(Some(raw)),
            RecordRead::Missing | RecordRead::Expired => Ok(None),
        },
    }
}

/// Every live row of one record store whose full key starts with `prefix`, as `(key, raw)`.
pub(super) fn scan(
    bounds: RecordBounds,
    prefix: &str,
    now_ms: i64,
) -> Result<Vec<(String, String)>, &'static str> {
    scan_record_raw(record_store(bounds), prefix, now_ms)
}

/// Writes one row, or removes it when `value` is `None`.
///
/// A value a catalog bound refuses comes back as `Err(<bound>)` (`entry`, `store`, `entries`,
/// `backend`), the same as a write that failed, and both are logged.
///
/// CDXC:Settings 2026-10-23 WHY:
/// A refused record write used to answer `Ok`, so the chat believed every refused row was saved,
/// counted nothing in `storageRefused`, and nobody could see why `sessionOptions` and
/// `chatSnapshots` had stopped taking new rows at their caps for weeks. The refusal now reaches the
/// core the way a failure does (`Event::StorageWritten { error }`), and every refusal or failure is
/// logged without the debug switch, at most once a minute per store and reason with the count since
/// the last line, so a write that keeps being refused cannot fill the disk.
pub(super) fn write(
    store_id: &str,
    backend: Backend,
    name: &str,
    value: Option<&str>,
    now_ms: i64,
) -> Result<(), &'static str> {
    let result = write_row(backend, name, value, now_ms);
    if let Err((outcome, reason)) = result {
        note_failed_write(store_id, outcome, reason);
    }
    result.map_err(|(_, reason)| reason)
}

/// The write itself, with whether a failure was a catalog refusal or the database failing.
fn write_row(
    backend: Backend,
    name: &str,
    value: Option<&str>,
    now_ms: i64,
) -> Result<(), (&'static str, &'static str)> {
    let failed = |reason| ("failed", reason);
    match backend {
        Backend::Local => crate::app::gx_store::with_write_connection(|connection| {
            match value {
            Some(raw) => connection
                .execute(
                    "INSERT INTO preferences VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    rusqlite::params![name, raw],
                )
                .map(|_| ())
                .map_err(|_| "write"),
            None => connection
                .execute("DELETE FROM preferences WHERE key=?1", [name])
                .map(|_| ())
                .map_err(|_| "write"),
        }
        })
        .map_err(failed),
        Backend::Records(bounds) => match value {
            Some(raw) => match write_record(record_store(bounds), name, raw, now_ms) {
                Ok(RecordWrite::Stored | RecordWrite::Unchanged) => Ok(()),
                Ok(RecordWrite::Refused(bound)) => Err(("refused", bound)),
                Err(reason) => Err(failed(reason)),
            },
            // `removeItem`, which is a real DELETE. An emptied row is not the same as an absent one
            // on this table: `remove_record` says why, and what it costs the TypeScript reader.
            None => remove_record(name).map_err(failed),
        },
    }
}

/// One store's refused or failed writes since its last log line.
struct FailedWrites {
    store: String,
    reason: &'static str,
    logged_at: Instant,
    unlogged: u64,
}

/// Logs a refused or failed write: the first of a kind at once, then at most one line a minute per
/// store and reason, carrying how many there were. The store id is the catalog's and the reason a
/// code constant; the key, which carries a session id, is never logged.
fn note_failed_write(store_id: &str, outcome: &'static str, reason: &'static str) {
    static FAILED: Mutex<Vec<FailedWrites>> = Mutex::new(Vec::new());
    let now = Instant::now();
    let writes = {
        let mut failed = FAILED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match failed
            .iter_mut()
            .find(|entry| entry.store == store_id && entry.reason == reason)
        {
            Some(entry) => {
                entry.unlogged += 1;
                if now.duration_since(entry.logged_at) < FAILED_WRITE_LOG_INTERVAL {
                    return;
                }
                entry.logged_at = now;
                std::mem::take(&mut entry.unlogged)
            }
            None => {
                if failed.len() >= MAX_FAILED_WRITE_KINDS {
                    failed.remove(0);
                }
                failed.push(FailedWrites {
                    store: store_id.to_string(),
                    reason,
                    logged_at: now,
                    unlogged: 0,
                });
                1
            }
        }
    };
    // "warning" in the event name is what makes `support_logs` write it without the debug switch.
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::SessionChat,
        "gxChat.storage.write.warning",
        serde_json::json!({
            "store": store_id,
            "outcome": outcome,
            "reason": reason,
            "writes": writes,
        }),
    );
}
