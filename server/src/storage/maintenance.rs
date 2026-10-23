use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use rusqlite::Connection;

use super::open_unpooled_gxserver_database;
use crate::paths::GxserverPaths;

/// Free pages below this count are left alone: reclaiming them is not worth rewriting the file.
const VACUUM_MIN_FREE_PAGES: i64 = 2_048;

static HELD_DATABASE: OnceLock<Mutex<Connection>> = OnceLock::new();

/// CDXC:ServerDaemon 2026-10-23 WHY:
/// SQLite checkpoints the whole WAL into state.db, fsyncs it, and deletes state.db-wal/-shm whenever the last connection closes. Storage operations borrow pooled connections now (`GxserverDatabase`), but the pool starts empty and closes the connections it cannot keep, so without one connection that outlives them all an operation could still pay that full cycle (it paid it several times per second while every operation opened its own connection). Holding this connection for the process lifetime keeps the WAL in place and leaves checkpointing to SQLite's auto-checkpoint. It is never used for queries.
pub fn hold_gxserver_database_open(paths: &GxserverPaths) -> Result<()> {
    if HELD_DATABASE.get().is_some() {
        return Ok(());
    }
    let db = open_unpooled_gxserver_database(&paths.state_db_file, None)?;
    let _ = HELD_DATABASE.set(Mutex::new(db));
    Ok(())
}

/// Returns freed pages to the filesystem once they make up a quarter of the file. Deleted rows only move pages to the freelist, so a large prune (consumed draft revisions) would otherwise never shrink state.db.
pub fn reclaim_free_database_pages(db: &Connection) -> Result<bool> {
    let free_pages: i64 = db.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    let total_pages: i64 = db.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    if free_pages < VACUUM_MIN_FREE_PAGES || free_pages * 4 < total_pages {
        return Ok(false);
    }
    db.execute_batch("VACUUM")?;
    Ok(true)
}
