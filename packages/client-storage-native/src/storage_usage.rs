//! The `records` table's per-store usage, kept exact across the writes of one connection instead
//! of being rescanned on every write.
//!
//! CDXC:Settings 2026-10-23 WHY:
//! Supersedes the per-write rescan of 2026-09-21, where every changed write ran
//! `scan_record_usage` (an aggregation over every row's JSON) inside `BEGIN IMMEDIATE`. At this
//! user's scale the table is about 12 MB of JSON and the scan about 13 ms, and the chat writes
//! several rows a second, so the bound checks cost more than the writes did. The tallies are now
//! scanned once and then moved by each write's own delta, the same arithmetic the write path
//! already trusted inside one transaction, carried across transactions. What made the incremental
//! scheme of `applyDatabaseMutations` drift was a SECOND writer the counters never heard about, so
//! the tallies carry a stamp: the connection's `PRAGMA data_version`, which moves when any OTHER
//! connection commits, and its `total_changes()`, which moves when any statement on THIS
//! connection changes a row behind this type's back. A stamp that no longer matches is a rescan, so
//! a missed change cannot outlive the next write and the bounds and the metadata rows stay exact.
//!
//! SEE-ALSO: packages/client-storage-native/src/storage_records.rs (the write and the removal that
//! move the tallies), packages/client-storage-native/src/storage_metadata.rs (the scan and the
//! metadata rows), apps/desktop/src/app/gx_store/sidebar_ui_storage.rs (the pool that keeps one
//! per write connection, and the `preferences` doors that carry the stamp past their own writes).

use rusqlite::{Connection, params};
use serde_json::json;

use crate::storage_metadata::{RecordStoreUsage, apply_record_metadata, scan_record_usage};

/// What the tallies were measured against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    data_version: i64,
    total_changes: i64,
}

/// Every store's bytes, entries and highest revision as one connection last saw them.
///
/// Belongs to ONE connection: the stamp means nothing on another, so a pool that opens a new
/// connection must start a new `RecordUsage` (or call [`RecordUsage::forget`]). A write or a
/// removal that fails forgets the tallies itself; a caller whose COMMIT fails must forget them too,
/// because the deltas were applied before it.
#[derive(Debug, Default)]
pub struct RecordUsage {
    stores: Vec<RecordStoreUsage>,
    /// The lowest `updatedAt` a store's rows can carry, for the stores a write has measured. A lower
    /// bound rather than the minimum: a row that leaves is not taken out of it, so the worst a
    /// stale value costs is one store scan that finds nothing to expire.
    oldest: Vec<(String, i64)>,
    stamp: Option<Stamp>,
    /// The metadata rows equal what `stores` implies. False after every scan until a write that
    /// rebuilt all of them, so a scan that found drift repairs every row and not only the ones the
    /// next write touches.
    metadata_synced: bool,
}

impl RecordUsage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops the tallies; the next write scans the table again.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    /// Whether the tallies still describe the table as this connection sees it.
    ///
    /// For a write on the same connection that changes no `records` or `metadata` row (the
    /// `preferences` doors): ask inside that write's immediate transaction, before its statements,
    /// and call [`RecordUsage::restamp`] after them when this was true.
    pub fn is_current(&self, connection: &Connection) -> bool {
        self.stamp.is_some() && read_stamp(connection).ok() == self.stamp
    }

    /// Moves the stamp past rows this connection just changed in a table the tallies do not count.
    /// Only after [`RecordUsage::is_current`] answered true inside the same immediate transaction,
    /// where no other connection can have committed in between.
    pub fn restamp(&mut self, connection: &Connection) {
        match read_stamp(connection) {
            Ok(stamp) => self.stamp = Some(stamp),
            Err(_) => self.forget(),
        }
    }

    /// Brings the tallies up to date: nothing when the stamp still matches, a full scan otherwise.
    pub(crate) fn refresh(&mut self, connection: &Connection) -> Result<(), &'static str> {
        let stamp = read_stamp(connection)?;
        if self.stamp == Some(stamp) {
            return Ok(());
        }
        self.stores = scan_record_usage(connection).map_err(|_| "usage")?;
        self.oldest.clear();
        self.metadata_synced = false;
        self.stamp = Some(stamp);
        Ok(())
    }

    pub(crate) fn stores(&self) -> &[RecordStoreUsage] {
        &self.stores
    }

    pub(crate) fn oldest(&self, store: &str) -> Option<i64> {
        self.oldest
            .iter()
            .find(|(held, _)| held == store)
            .map(|(_, oldest)| *oldest)
    }

    pub(crate) fn set_oldest(&mut self, store: &str, oldest: Option<i64>) {
        self.oldest.retain(|(held, _)| held != store);
        if let Some(oldest) = oldest {
            self.oldest.push((store.to_string(), oldest));
        }
    }

    /// One row arrived in `store`.
    pub(crate) fn add_row(&mut self, store: &str, bytes: i64, revision: i64) {
        match self.stores.iter_mut().find(|entry| entry.store == store) {
            Some(entry) => {
                entry.bytes += bytes;
                entry.entries += 1;
                entry.highest_revision = entry.highest_revision.max(revision);
            }
            None => self.stores.push(RecordStoreUsage {
                store: store.to_string(),
                bytes,
                entries: 1,
                highest_revision: revision,
            }),
        }
    }

    /// One row left `store`.
    ///
    /// `highest_revision` is left where it was even when the row carried it: the only thing read
    /// from it is the floor the next revision is taken above, and a floor that is too high costs
    /// nothing. A store left with no rows is dropped, as a scan would not report it, and its
    /// metadata row is written back to zero by [`RecordUsage::settle`].
    pub(crate) fn remove_row(&mut self, store: &str, bytes: i64) {
        if let Some(entry) = self.stores.iter_mut().find(|entry| entry.store == store) {
            entry.bytes -= bytes;
            entry.entries -= 1;
        }
        self.stores.retain(|entry| entry.entries > 0);
        if !self.stores.iter().any(|entry| entry.store == store) {
            self.set_oldest(store, None);
        }
    }

    /// Writes the metadata rows the moved tallies imply, inside the caller's transaction, and
    /// stamps the tallies with the connection as it now stands.
    ///
    /// Only `touched` stores, `total` and (when a row was written) `revision` can have changed
    /// since the rows were last in sync, so only they are written. The first write after a scan
    /// rebuilds every row through [`apply_record_metadata`] instead, which is what every write did
    /// before the tallies were kept.
    pub(crate) fn settle(
        &mut self,
        connection: &Connection,
        touched: &[&str],
        revision: Option<i64>,
    ) -> Result<(), &'static str> {
        if self.metadata_synced {
            for store in touched {
                let (bytes, entries) = self
                    .stores
                    .iter()
                    .find(|entry| entry.store == *store)
                    .map_or((0, 0), |entry| (entry.bytes, entry.entries));
                put_metadata(
                    connection,
                    store,
                    &json!({"bytes": bytes, "entries": entries}).to_string(),
                )?;
            }
            let total: i64 = self.stores.iter().map(|entry| entry.bytes).sum();
            put_metadata(connection, "total", &json!(total).to_string())?;
            if let Some(revision) = revision {
                put_metadata(connection, "revision", &json!(revision).to_string())?;
            }
        } else {
            apply_record_metadata(connection, &self.stores).map_err(|_| "metadata")?;
            self.metadata_synced = true;
        }
        self.stamp = Some(read_stamp(connection)?);
        Ok(())
    }
}

fn put_metadata(connection: &Connection, key: &str, value: &str) -> Result<(), &'static str> {
    connection
        .prepare_cached(
            "INSERT INTO metadata VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        )
        .and_then(|mut statement| statement.execute(params![key, value]))
        .map(|_| ())
        .map_err(|_| "metadata")
}

fn read_stamp(connection: &Connection) -> Result<Stamp, &'static str> {
    connection
        .prepare_cached("SELECT data_version, total_changes() FROM pragma_data_version")
        .and_then(|mut statement| {
            statement.query_row([], |row| {
                Ok(Stamp {
                    data_version: row.get(0)?,
                    total_changes: row.get(1)?,
                })
            })
        })
        .map_err(|_| "usage")
}
