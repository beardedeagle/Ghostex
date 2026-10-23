-- CDXC:StateSync 2026-10-23 WHY:
-- The background tasks and the chat follower list sessions by lifecycle state (`running`, not
-- `stopped`) every few seconds, and without this index each list read every row of the registry
-- (10,000+ stopped rows next to a few hundred live ones). The user chose the index despite the
-- small write it adds to every session update (see `apply_wal_durability` in storage/setup.rs).
CREATE INDEX IF NOT EXISTS idx_sessions_lifecycle_updated ON sessions(lifecycleState, updatedAt);

PRAGMA user_version = 46;
