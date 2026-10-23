//! The desktop's `gx_chat/storage_backend.rs` for a phone: the chat's rows in a
//! `client-storage.sqlite3` of the phone's own, in the data directory the host passed to
//! [`crate::mobile::init`].
//!
//! The file has the desktop's shape and rules because it is made and written by the same crate,
//! `packages/client-storage-native`: [`ghostex_client_storage::initialize_client_storage`] creates
//! the tables, a `local` catalog row lives in `preferences` as `(key, value)`, and an `indexeddb`
//! row lives in `records` through the crate's own `read_record` / `write_record`, which apply the
//! catalog's four bounds and keep the table's metadata. What differs from the desktop is only the
//! connection: the desktop shares a pool with its sidebar stores, and here the chat host's one
//! thread is the file's only user, so one connection behind a mutex is the whole door.
//!
//! The prefix scan and the real DELETE follow `apps/desktop/src/app/gx_store/records_storage.rs`
//! (`scan_record_raw`, `remove_record`), which say why each is shaped as it is.

use std::sync::{Mutex, OnceLock};

use ghostex_client_storage::{RecordRead, RecordStore, RecordWrite};
use rusqlite::{Connection, OptionalExtension as _};

use super::storage::{Backend, RecordBounds};

static CONNECTION: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();

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

/// Runs `body` on the one connection, opening (and initializing) the file on first use. A failed
/// open is retried on the next call rather than remembered, so a data directory the host creates
/// late still works.
fn with_connection<T>(
    body: impl FnOnce(&Connection) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let cell = CONNECTION.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().map_err(|_| "lock")?;
    if guard.is_none() {
        // Set by `mobile::init` before any chat opens.
        let dir = crate::mobile::data_dir();
        if dir.as_os_str().is_empty() {
            return Err("no data directory");
        }
        std::fs::create_dir_all(&dir).map_err(|_| "open")?;
        let path = dir.join("client-storage.sqlite3");
        ghostex_client_storage::initialize_client_storage(
            &path,
            None,
            super::platform::now_millis(),
        )
        .map_err(|_| "open")?;
        let connection = Connection::open(&path).map_err(|_| "open")?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| "busy")?;
        *guard = Some(connection);
    }
    let result = body(guard.as_ref().ok_or("open")?);
    // A failed statement may leave a transaction open; drop the connection so the next call starts
    // clean, the rule `finish_without_writing` on the desktop keeps for its pool.
    if result.is_err() {
        if let Some(connection) = guard.as_ref() {
            if !connection.is_autocommit() {
                *guard = None;
            }
        }
    }
    result
}

/// One row's raw value, `None` when it is absent or expired.
pub(super) fn read(
    _store_id: &str,
    backend: Backend,
    name: &str,
    now_ms: i64,
) -> Result<Option<String>, &'static str> {
    with_connection(|connection| match backend {
        Backend::Local => connection
            .query_row(
                "SELECT value FROM preferences WHERE key=?1",
                [name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| "query"),
        Backend::Records(bounds) => {
            match ghostex_client_storage::read_record(
                connection,
                record_store(bounds),
                name,
                now_ms,
            )? {
                RecordRead::Payload(raw) => Ok(Some(raw)),
                RecordRead::Missing | RecordRead::Expired => Ok(None),
            }
        }
    })
}

/// Every live row of one record store whose full key starts with `prefix`, as `(key, raw)`.
pub(super) fn scan(
    bounds: RecordBounds,
    prefix: &str,
    now_ms: i64,
) -> Result<Vec<(String, String)>, &'static str> {
    let upper = prefix_successor(prefix);
    with_connection(|connection| {
        let mut statement = match &upper {
            Some(_) => connection
                .prepare("SELECT key, value FROM records WHERE store=?1 AND key>=?2 AND key<?3"),
            None => connection.prepare("SELECT key, value FROM records WHERE store=?1"),
        }
        .map_err(|_| "read")?;
        let bind: Vec<&dyn rusqlite::ToSql> = match &upper {
            Some(upper) => vec![&bounds.id, &prefix, upper],
            None => vec![&bounds.id],
        };
        let rows = statement
            .query_map(rusqlite::params_from_iter(bind), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| "read")?;
        let mut found = Vec::new();
        for row in rows {
            let (key, value) = row.map_err(|_| "read")?;
            if !key.starts_with(prefix) {
                continue;
            }
            let Ok(row) = serde_json::from_str::<serde_json::Value>(&value) else {
                continue;
            };
            let Some(raw) = row.get("raw").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let updated_at = row
                .get("updatedAt")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default();
            if bounds
                .max_age_ms
                .is_some_and(|limit| now_ms - updated_at > limit)
            {
                continue;
            }
            found.push((key, raw.to_string()));
        }
        Ok(found)
    })
}

/// The first key that sorts after every key starting with `prefix` under SQLite's BINARY collation.
fn prefix_successor(prefix: &str) -> Option<String> {
    let mut bytes = prefix.as_bytes().to_vec();
    while let Some(last) = bytes.pop() {
        if last < 0xff {
            bytes.push(last + 1);
            return String::from_utf8(bytes).ok();
        }
    }
    None
}

/// Writes one row, or removes it when `value` is `None`.
pub(super) fn write(
    _store_id: &str,
    backend: Backend,
    name: &str,
    value: Option<&str>,
    now_ms: i64,
) -> Result<(), &'static str> {
    with_connection(|connection| {
        match (backend, value) {
        (Backend::Local, Some(raw)) => connection
            .execute(
                "INSERT INTO preferences VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                rusqlite::params![name, raw],
            )
            .map(|_| ())
            .map_err(|_| "write"),
        (Backend::Local, None) => connection
            .execute("DELETE FROM preferences WHERE key=?1", [name])
            .map(|_| ())
            .map_err(|_| "write"),
        (Backend::Records(bounds), Some(raw)) => {
            connection.execute_batch("BEGIN IMMEDIATE").map_err(|_| "begin")?;
            let result = ghostex_client_storage::write_record(
                connection,
                record_store(bounds),
                name,
                raw,
                now_ms,
            );
            finish(connection, matches!(result, Ok(RecordWrite::Stored)))?;
            // A refused write reaches the core as a failed write, as the desktop's door reports it
            // (`write_row` in `apps/desktop/src/app/gx_chat/storage_backend.rs`); it used to look
            // like success.
            match result {
                Ok(RecordWrite::Stored | RecordWrite::Unchanged) => Ok(()),
                Ok(RecordWrite::Refused(_)) => Err("refused"),
                Err(reason) => Err(reason),
            }
        }
        (Backend::Records(_), None) => {
            connection.execute_batch("BEGIN IMMEDIATE").map_err(|_| "begin")?;
            let removed = match connection.execute("DELETE FROM records WHERE key=?1", [name]) {
                Ok(removed) => removed,
                Err(_) => {
                    finish(connection, false)?;
                    return Err("write");
                }
            };
            if removed == 0 {
                return finish(connection, false);
            }
            if ghostex_client_storage::recompute_record_metadata(connection).is_err() {
                finish(connection, false)?;
                return Err("metadata");
            }
            finish(connection, true)
        }
    }
    })
}

/// Commits, or rolls back a transaction that wrote nothing.
fn finish(connection: &Connection, commit: bool) -> Result<(), &'static str> {
    connection
        .execute_batch(if commit { "COMMIT" } else { "ROLLBACK" })
        .map_err(|_| if commit { "commit" } else { "rollback" })
}
