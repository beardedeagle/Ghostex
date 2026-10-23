//! Writing one row of the `records` table: the catalog's bounds, the row shape and the metadata
//! the table's writers owe, with no connection, pool or transaction of its own.
//!
//! CDXC:Settings 2026-09-21 WHY:
//! It lives in this crate rather than beside its caller in `apps/desktop` so that it can be DRIVEN.
//! The desktop door is a process-global pool pointed at the user's own state directory, so the
//! first cut of this logic could only be gated by copying its body into a scratch crate outside the
//! repository, which is the shape this port has repeatedly found a bug hiding in: the copy and the
//! original drift and the gate goes on passing against the copy. Everything here takes a
//! `&Connection` and the caller owns the transaction, so `examples/records_door.rs` runs the real
//! code against a COPY of a real database.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/records_storage.rs (the pool, the transaction and the
//! error vocabulary), packages/client-storage/adapters/database-transaction.ts (the other writer of
//! this table), packages/client-storage-native/src/storage_metadata.rs (the bookkeeping).

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::storage_catalog::catalog_store;
use crate::storage_metadata::RecordStoreUsage;
use crate::storage_usage::RecordUsage;

/// `STORAGE_BUDGETS.indexeddb` (`packages/client-storage/budgets.ts`), shared by every store on
/// that backend.
pub const MAX_BACKEND_BYTES: i64 = 128 * 1024 * 1024;

/// One store's row in the catalog, as far as a read or a write needs it.
#[derive(Clone, Copy, Debug)]
pub struct RecordStore {
    /// The catalog `id`, which is also the `store` column and the key of the store's `metadata`
    /// row.
    pub id: &'static str,
    /// The catalog `version`, stored on the row as `schemaVersion`.
    pub version: i64,
    pub max_entry_bytes: i64,
    pub max_bytes: i64,
    pub max_entries: i64,
    /// The catalog `maxAgeMs`, for a `cache` store that has one. `None` is a store that keeps its
    /// rows for ever.
    pub max_age_ms: Option<i64>,
}

/// What one write did. `Refused` names the bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordWrite {
    Stored,
    /// The row already held exactly this payload at this schema version, so nothing was written.
    /// `applyDatabaseMutations` skips the same case, which is what keeps two writers of one key
    /// from flipping its `updatedAt` between them.
    Unchanged,
    Refused(&'static str),
}

/// The size the catalog measures a row by: UTF-16 code units of the key and the payload together,
/// two bytes each, which is `storageBytes` in `packages/client-storage/budgets.ts`.
pub fn storage_bytes(key: &str, raw: &str) -> usize {
    2 * (utf16_len(key) + utf16_len(raw))
}

/// The length JavaScript measures, which is code units rather than characters or bytes.
fn utf16_len(value: &str) -> usize {
    value.chars().map(char::len_utf16).sum()
}

/// Stores one record, with the catalog's bounds and the table's bookkeeping, measured by a scan of
/// its own. A caller that writes more than once keeps a [`RecordUsage`] and calls
/// [`RecordUsage::write`], which scans once and then keeps the numbers.
///
/// The caller owns the transaction and must already be inside one: the bounds are measured against
/// the table as it stands and the row and its metadata have to land together.
pub fn write_record(
    connection: &Connection,
    store: RecordStore,
    key: &str,
    raw: &str,
    now_ms: i64,
) -> Result<RecordWrite, &'static str> {
    RecordUsage::new().write(connection, store, key, raw, now_ms)
}

impl RecordUsage {
    /// Stores one record, with the catalog's bounds and the table's bookkeeping.
    ///
    /// The caller owns the transaction and must already be inside one (`BEGIN IMMEDIATE`), commit
    /// it on `Stored`, roll it back otherwise, and forget these tallies when its COMMIT fails.
    ///
    /// CDXC:Settings 2026-10-23 WHY:
    /// Supersedes the 2026-09-21 cost model of one pass over the whole table per changed write.
    /// The order of the steps is the cost model. The UNCHANGED comparison is first and is one
    /// indexed lookup, because the common case is a writer repeating the payload it already
    /// stored, and it must not pay for anything else. The bounds are then measured against the
    /// tallies, which cost nothing while their stamp holds (`storage_usage.rs`). Only a `cache`
    /// store that is at a bound, or may hold an expired row, reads its own rows, because that is
    /// the only case that has to choose rows to evict.
    pub fn write(
        &mut self,
        connection: &Connection,
        store: RecordStore,
        key: &str,
        raw: &str,
        now_ms: i64,
    ) -> Result<RecordWrite, &'static str> {
        let result = self.write_row(connection, store, key, raw, now_ms);
        if result.is_err() {
            self.forget();
        }
        result
    }

    /// Removes one record and keeps the bookkeeping exact. `Ok(false)` is a key with no row, and
    /// nothing was written; the caller owns the transaction as for [`RecordUsage::write`].
    ///
    /// The `revision` counter is left where it is: nothing a removal does can be compared against
    /// it, and the full recount this replaced could move it backwards past its own rows.
    pub fn remove(&mut self, connection: &Connection, key: &str) -> Result<bool, &'static str> {
        let result = self.remove_row_at(connection, key);
        if result.is_err() {
            self.forget();
        }
        result
    }

    fn write_row(
        &mut self,
        connection: &Connection,
        store: RecordStore,
        key: &str,
        raw: &str,
        now_ms: i64,
    ) -> Result<RecordWrite, &'static str> {
        let held: Option<(String, String)> = connection
            .prepare_cached("SELECT store, value FROM records WHERE key=?1")
            .and_then(|mut statement| {
                statement
                    .query_row([key], |row| Ok((row.get(0)?, row.get(1)?)))
                    .optional()
            })
            .map_err(|_| "read")?;
        let held: Option<(String, Value)> = held
            .and_then(|(store, text)| Some((store, serde_json::from_str::<Value>(&text).ok()?)));
        let unchanged = held.as_ref().is_some_and(|(_, row)| {
            row.get("raw").and_then(Value::as_str) == Some(raw)
                && row.get("schemaVersion").and_then(Value::as_i64) == Some(store.version)
        });
        if unchanged {
            return Ok(RecordWrite::Unchanged);
        }
        self.refresh(connection)?;
        let previous_store = held.as_ref().map(|(store, _)| store.clone());
        let previous_bytes = held.as_ref().map_or(0, |(_, row)| row_number(row, "bytes"));
        let usage = Usage::from_scan(
            self.stores(),
            store.id,
            previous_store.as_deref(),
            previous_bytes,
        );
        let next_bytes = storage_bytes(key, raw) as i64;
        let mut eviction = Vec::new();
        let mut measured_oldest = None;
        if catalog_store(store.id).is_some_and(|definition| definition.cache) {
            if next_bytes > store.max_entry_bytes {
                return Ok(RecordWrite::Refused("entry"));
            }
            if usage.bound_broken(store, next_bytes).is_some()
                || self.may_hold_expired(store, &usage, now_ms)
            {
                match plan_eviction(connection, store, key, next_bytes, now_ms, &usage)? {
                    Eviction::Refused(bound) => return Ok(RecordWrite::Refused(bound)),
                    Eviction::Admitted { removals, oldest } => {
                        eviction = removals;
                        measured_oldest = Some(oldest);
                    }
                }
            }
        } else if let Some(bound) = usage.refuses(store, next_bytes) {
            return Ok(RecordWrite::Refused(bound));
        }
        // The counter `applyDatabaseMutations` bumps per mutation. Taken above BOTH the metadata row
        // and every row's own revision, so the bookkeeping, which reports the highest row revision,
        // cannot move the stored counter backwards past a service that is ahead of its rows.
        let revision = usage.revision_floor.max(metadata_revision(connection)?) + 1;
        for (evicted, bytes) in &eviction {
            connection
                .prepare_cached("DELETE FROM records WHERE key=?1")
                .and_then(|mut statement| statement.execute([evicted]))
                .map_err(|_| "write")?;
            self.remove_row(store.id, *bytes);
        }
        let row = json!({
            "key": key,
            "store": store.id,
            "raw": raw,
            "bytes": next_bytes,
            "updatedAt": now_ms,
            "revision": revision,
            "schemaVersion": store.version,
        });
        connection
            .prepare_cached(
                "INSERT INTO records VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET store=excluded.store,value=excluded.value",
            )
            .and_then(|mut statement| statement.execute(params![key, store.id, row.to_string()]))
            .map_err(|_| "write")?;
        let known_oldest = self.oldest(store.id);
        let was_empty = usage.store_entries == 0;
        if let Some(previous_store) = previous_store.as_deref() {
            self.remove_row(previous_store, previous_bytes);
        }
        self.add_row(store.id, next_bytes, revision);
        if store.max_age_ms.is_some() {
            let oldest = match measured_oldest {
                Some(measured) => measured,
                None if was_empty => Some(now_ms),
                None => known_oldest.map(|oldest| oldest.min(now_ms)),
            };
            self.set_oldest(store.id, oldest);
        }
        let mut touched = vec![store.id];
        if let Some(previous_store) = previous_store.as_deref()
            && previous_store != store.id
        {
            touched.push(previous_store);
        }
        self.settle(connection, &touched, Some(revision))?;
        Ok(RecordWrite::Stored)
    }

    fn remove_row_at(&mut self, connection: &Connection, key: &str) -> Result<bool, &'static str> {
        // Measured the way the scan measures it, so the tallies move by what the scan counted.
        let held: Option<(String, i64)> = connection
            .prepare_cached(
                "SELECT store, CAST(COALESCE(json_extract(value,'$.bytes'),0) AS INTEGER) FROM records WHERE key=?1",
            )
            .and_then(|mut statement| {
                statement
                    .query_row([key], |row| Ok((row.get(0)?, row.get(1)?)))
                    .optional()
            })
            .map_err(|_| "read")?;
        let Some((store, bytes)) = held else {
            return Ok(false);
        };
        // Before the delete, so a scan this has to take still counts the row it removes.
        self.refresh(connection)?;
        connection
            .prepare_cached("DELETE FROM records WHERE key=?1")
            .and_then(|mut statement| statement.execute([key]))
            .map_err(|_| "write")?;
        self.remove_row(&store, bytes);
        self.settle(connection, &[store.as_str()], None)?;
        Ok(true)
    }

    /// Whether a `cache` store with an age limit may hold a row past it, which `admission` would
    /// evict on this write even with every bound met.
    fn may_hold_expired(&self, store: RecordStore, usage: &Usage, now_ms: i64) -> bool {
        let Some(limit) = store.max_age_ms else {
            return false;
        };
        if usage.store_entries == 0 {
            return false;
        }
        self.oldest(store.id)
            .is_none_or(|oldest| now_ms - oldest > limit)
    }
}

/// What `admission` decided for a write to a `cache` store.
enum Eviction {
    /// The rows to delete first, as `(key, bytes)`, and the oldest `updatedAt` left behind.
    Admitted {
        removals: Vec<(String, i64)>,
        oldest: Option<i64>,
    },
    Refused(&'static str),
}

/// `admission` (`packages/client-storage/budgets.ts`) for a `cache` store: its expired rows, and
/// then its oldest rows, give way until the write fits, the oldest first by `updatedAt` and then by
/// key.
///
/// CDXC:Settings 2026-10-23 WHY:
/// Supersedes the 2026-09-21 rule that this door refused every write that broke a bound, even in
/// a `cache` store. A cache store's own rows are disposable by the catalog's definition, and
/// refusing instead froze each store at its cap: `sessionOptions` sat at 2,000 rows (1,692 of them
/// past their 30 days) and refused the options of every new session, and `chatSnapshots` sat at 24
/// expired rows and refused every new retained conversation. What stays refused is the part of
/// `applyDatabaseMutations` that evicts OTHER stores' cache rows when the shared 128 MiB total is
/// what broke, because a write that quietly deleted another feature's data would be a loss nobody
/// could trace back to it, and a write whose own row would be the one to give way, which
/// `admission` turns into a silent delete. Both are refused and named instead.
fn plan_eviction(
    connection: &Connection,
    store: RecordStore,
    key: &str,
    next_bytes: i64,
    now_ms: i64,
    usage: &Usage,
) -> Result<Eviction, &'static str> {
    let mut candidates: Vec<(String, i64, i64)> = connection
        .prepare_cached(
            "SELECT key,
                    CAST(COALESCE(json_extract(value,'$.bytes'),0) AS INTEGER),
                    CAST(COALESCE(json_extract(value,'$.updatedAt'),0) AS INTEGER)
             FROM records WHERE store=?1",
        )
        .and_then(|mut statement| {
            statement
                .query_map([store.id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|_| "usage")?;
    candidates.retain(|(held, _, _)| held != key);
    let mut bytes = candidates.iter().map(|(_, bytes, _)| bytes).sum::<i64>() + next_bytes;
    let mut count = candidates.len() as i64 + 1;
    let mut total = usage.backend_bytes - usage.previous_bytes + next_bytes;
    candidates.push((key.to_string(), next_bytes, now_ms));
    candidates.sort_by(|left, right| left.2.cmp(&right.2).then_with(|| left.0.cmp(&right.0)));
    let expired = |updated_at: i64| {
        store
            .max_age_ms
            .is_some_and(|limit| now_ms - updated_at > limit)
    };
    let mut removals = Vec::new();
    let mut kept_from = candidates.len();
    for (index, (candidate, candidate_bytes, updated_at)) in candidates.iter().enumerate() {
        let broken = cache_bound_broken(store, bytes, count, total);
        if broken.is_none() && !expired(*updated_at) {
            kept_from = index;
            break;
        }
        if candidate == key {
            return Ok(Eviction::Refused(broken.unwrap_or("store")));
        }
        removals.push((candidate.clone(), *candidate_bytes));
        bytes -= candidate_bytes;
        total -= candidate_bytes;
        count -= 1;
    }
    if let Some(bound) = cache_bound_broken(store, bytes, count, total) {
        return Ok(Eviction::Refused(bound));
    }
    let oldest = candidates[kept_from..]
        .iter()
        .map(|(_, _, updated_at)| *updated_at)
        .min();
    Ok(Eviction::Admitted { removals, oldest })
}

/// `admission`'s three totals, in its order.
fn cache_bound_broken(
    store: RecordStore,
    bytes: i64,
    count: i64,
    total: i64,
) -> Option<&'static str> {
    if bytes > store.max_bytes {
        Some("store")
    } else if count > store.max_entries {
        Some("entries")
    } else if total > MAX_BACKEND_BYTES {
        Some("backend")
    } else {
        None
    }
}

/// A number off a stored row, read the way SQLite's `CAST(... AS INTEGER)` reads it, so the
/// arithmetic above and the scan's SQL agree on a row whose field is a string or a float.
fn row_number(row: &Value, field: &str) -> i64 {
    match row.get(field) {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0) as i64,
        Some(Value::String(text)) => text.parse::<f64>().unwrap_or(0.0) as i64,
        _ => 0,
    }
}

/// The sizes the four bounds are measured against.
struct Usage {
    /// Bytes and entries of this store, NOT counting the key being written.
    store_bytes: i64,
    store_entries: i64,
    /// Bytes of every row on the backend, the key being written included.
    backend_bytes: i64,
    /// What the key being written holds today.
    previous_bytes: i64,
    /// The highest revision any row carries.
    revision_floor: i64,
}

impl Usage {
    fn from_scan(
        scan: &[RecordStoreUsage],
        store_id: &str,
        previous_store: Option<&str>,
        previous_bytes: i64,
    ) -> Self {
        let own = scan.iter().find(|entry| entry.store == store_id);
        let key_is_in_this_store = previous_store == Some(store_id);
        Self {
            store_bytes: own.map_or(0, |entry| entry.bytes)
                - if key_is_in_this_store {
                    previous_bytes
                } else {
                    0
                },
            store_entries: own.map_or(0, |entry| entry.entries) - i64::from(key_is_in_this_store),
            backend_bytes: scan.iter().map(|entry| entry.bytes).sum(),
            previous_bytes,
            revision_floor: scan
                .iter()
                .map(|entry| entry.highest_revision)
                .max()
                .unwrap_or(0),
        }
    }

    /// The bound this value would break, or `None` when all four admit it. `admission`'s four, in
    /// its order and in its UTF-16 accounting.
    fn bound_broken(&self, store: RecordStore, next_bytes: i64) -> Option<&'static str> {
        if next_bytes > store.max_entry_bytes {
            return Some("entry");
        }
        if self.store_bytes + next_bytes > store.max_bytes {
            return Some("store");
        }
        if self.store_entries + 1 > store.max_entries {
            return Some("entries");
        }
        (self.backend_bytes - self.previous_bytes + next_bytes > MAX_BACKEND_BYTES)
            .then_some("backend")
    }

    /// The refusal for a store that is not a `cache`: any broken bound, but only for a write that
    /// GROWS the key, which is `applyDatabaseMutations`' rule that existing protected data may
    /// exceed a new budget and shrinking it must still work.
    fn refuses(&self, store: RecordStore, next_bytes: i64) -> Option<&'static str> {
        if next_bytes <= self.previous_bytes {
            return None;
        }
        self.bound_broken(store, next_bytes)
    }
}

fn metadata_revision(connection: &Connection) -> Result<i64, &'static str> {
    let stored: Option<String> = connection
        .prepare_cached("SELECT value FROM metadata WHERE key='revision'")
        .and_then(|mut statement| statement.query_row([], |row| row.get(0)).optional())
        .map_err(|_| "metadata")?;
    Ok(stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_i64())
        .unwrap_or(0))
}

/// What one read found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordRead {
    Missing,
    /// The row is there and the catalog says it is too old to be used. Told apart from `Missing`
    /// because the two mean different things to a caller: nothing was ever stored, against a copy
    /// that aged out.
    Expired,
    Payload(String),
}

/// The `raw` payload of one record, if the catalog still admits it.
///
/// A row whose JSON does not parse, or that carries no `raw` string, reads as absent rather than
/// as an error: the callers of this table are caches, and one damaged entry must not stop the
/// others being read.
///
/// CDXC:Settings 2026-09-21 WHY:
/// **The catalog's `maxAgeMs` is part of the read, not only of eviction.** `readManaged`
/// (`packages/client-storage/service.ts`) answers `null` for a `cache` entry older than its store's
/// `maxAgeMs` before it ever looks at the payload, so a Rust door that handed back a two-month-old
/// row would have drawn data the TypeScript reader of the same key had already stopped drawing. The
/// one thing NOT copied is the deletion `readManaged` then performs: a Rust door refuses where the
/// service evicted, and a read never deletes. The clock is the caller's, because the
/// store this feeds has none of its own.
pub fn read_record(
    connection: &Connection,
    store: RecordStore,
    key: &str,
    now_ms: i64,
) -> Result<RecordRead, &'static str> {
    let row: Option<String> = connection
        .prepare_cached("SELECT value FROM records WHERE key=?1")
        .and_then(|mut statement| statement.query_row([key], |row| row.get(0)).optional())
        .map_err(|_| "read")?;
    let Some(row) = row.and_then(|text| serde_json::from_str::<Value>(&text).ok()) else {
        return Ok(RecordRead::Missing);
    };
    let Some(raw) = row.get("raw").and_then(Value::as_str) else {
        return Ok(RecordRead::Missing);
    };
    if store
        .max_age_ms
        .is_some_and(|limit| now_ms - row_number(&row, "updatedAt") > limit)
    {
        return Ok(RecordRead::Expired);
    }
    Ok(RecordRead::Payload(raw.to_string()))
}
