-- CDXC:StateSync 2026-10-23 WHY:
-- The sidebar's first snapshot lists every live session plus the stopped ones it keeps (pinned,
-- parked, favorite or tagged). Live rows come from idx_sessions_lifecycle_updated, but the kept
-- stopped rows had no index, so each snapshot still read all 10,000+ rows of the registry. This
-- partial index holds only the kept rows. The user chose it despite the small write it adds to
-- every session update (see `apply_wal_durability` in storage/setup.rs).
CREATE INDEX IF NOT EXISTS idx_sessions_presentation_kept ON sessions(updatedAt)
  WHERE isPinned = 1 OR isParked = 1 OR isFavorite = 1 OR (sessionTag IS NOT NULL AND sessionTag <> '');

PRAGMA user_version = 47;
