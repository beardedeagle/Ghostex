use super::*;

pub struct Migration {
    pub id: &'static str,
    pub sql: &'static str,
}

#[derive(Clone, Debug)]
pub struct StorageInitResult {
    pub applied_migrations: Vec<String>,
    pub state_db_file: String,
}

pub(super) const LEGACY_MACOS_STATE_IMPORT_ID: &str = "legacy_macos_sidebar_state_v1";
pub(super) const LEGACY_IMPORT_METADATA_KEY: &str = "migration.legacy_macos_sidebar_state_v1";
pub(super) const LEGACY_RECENT_PROJECTS_BACKFILL_ID: &str = "legacy_macos_recent_projects_v1";
pub(super) const LEGACY_RECENT_PROJECTS_BACKFILL_METADATA_KEY: &str =
    "migration.legacy_macos_recent_projects_v1";
pub(super) const LEGACY_NATIVE_PROJECTS_STATE_FILE: &str = "native-sidebar-projects.json";

/*
CDXC:ServerDaemon 2026-06-14-20:37:
SQLite remains TypeScript-compatible during the Rust port. Open every connection with foreign_keys=ON and journal_mode=WAL, then apply migration IDs 0001 through 0015 without inventing a parallel schema.

CDXC:ServerDaemon 2026-06-24-13:30:
Pinned Prompts are a shared user-data surface, not GPUI-local modal state. Store their content in gxserver SQLite behind explicit product-data RPCs so every client hydrates the same React contract without logging prompt bodies.
*/
pub fn initialize_gxserver_storage(paths: &GxserverPaths) -> Result<StorageInitResult> {
    ensure_gxserver_storage_layout(paths)?;
    let mut db = open_gxserver_database(paths)?;
    let applied_migrations = run_gxserver_migrations(&mut db)?;
    backfill_legacy_macos_recent_projects(&mut db, paths)?;
    maintenance::reclaim_free_database_pages(&db)?;
    Ok(StorageInitResult {
        applied_migrations,
        state_db_file: paths.state_db_file.to_string_lossy().to_string(),
    })
}

pub fn create_gxserver_migration_status(result: &StorageInitResult) -> MigrationStatus {
    MigrationStatus {
        applied_migrations: result.applied_migrations.clone(),
        current_version: GXSERVER_MIGRATION_IDS.len(),
        state_db_file: result.state_db_file.clone(),
        state_imports: Some(MigrationStateImports {
            legacy_macos_state: read_existing_legacy_import_status(&result.state_db_file)
                .unwrap_or_else(default_no_legacy_state_import_status),
        }),
    }
}

fn default_no_legacy_state_import_status() -> LegacyMacosStateImportStatus {
    LegacyMacosStateImportStatus {
        completed_at: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        id: LEGACY_MACOS_STATE_IMPORT_ID.to_string(),
        logs_imported: Some(LegacyMacosLogsImportStatus {
            files_read: 0,
            malformed_line_count: 0,
            migrated_line_count: 0,
        }),
        projects_imported: Some(0),
        sessions_imported: Some(0),
        skipped_reason: Some("noLegacyState".to_string()),
        source_files_read: Some(Vec::new()),
        status: "skipped".to_string(),
    }
}

/*
CDXC:ServerDaemon 2026-06-22-05:10:
Existing TypeScript-created state.db files can already contain the legacy macOS import marker in metadata. Rust startup must surface that durable marker as TypeScript does on later launches: completed markers report `skipped` with `alreadyCompleted`, while missing or non-completed markers continue through the no-legacy startup status path.
*/
fn read_existing_legacy_import_status(state_db_file: &str) -> Option<LegacyMacosStateImportStatus> {
    let db = Connection::open(Path::new(state_db_file)).ok()?;
    let value: String = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [LEGACY_IMPORT_METADATA_KEY],
            |row| row.get(0),
        )
        .optional()
        .ok()??;
    let mut status: LegacyMacosStateImportStatus = serde_json::from_str(&value).ok()?;
    if status.status != "completed" {
        return None;
    }
    status.id = LEGACY_MACOS_STATE_IMPORT_ID.to_string();
    status.status = "skipped".to_string();
    status.skipped_reason = Some("alreadyCompleted".to_string());
    Some(status)
}

pub fn ensure_gxserver_storage_layout(paths: &GxserverPaths) -> Result<()> {
    let config_dir = paths
        .config_file
        .parent()
        .context("resolve gxserver config directory")?;
    fs::create_dir_all(config_dir).with_context(|| "create gxserver config directory")?;
    fs::create_dir_all(&paths.auth_dir).with_context(|| "create auth directory")?;
    set_dir_mode_0700(&paths.auth_dir)?;
    fs::create_dir_all(&paths.logs_dir).with_context(|| "create logs directory")?;
    fs::create_dir_all(&paths.migrations_dir).with_context(|| "create migrations directory")?;
    fs::create_dir_all(&paths.runtime_dir).with_context(|| "create runtime directory")?;
    fs::create_dir_all(&paths.zmx_dir).with_context(|| "create zmx directory")?;
    write_default_config_if_missing(paths)?;
    Ok(())
}

/// CDXC:ServerDaemon 2026-10-23 DECISION:
/// User: "avoid any unnecessary writes", and accepted synchronous=NORMAL for state.db. In WAL mode this drops the fsync on every commit and syncs at checkpoints instead: an app or gxserver crash still loses nothing and the database cannot corrupt; only a power cut or OS crash can lose the last few commits.
/// On 2026-10-23 the user chose the sessions(lifecycleState, updatedAt) index (migration 0046) despite the small write it adds to every session update, so the lifecycle lists stop reading the whole registry; this supersedes reading "no unnecessary writes" as "no new indexes".
pub(super) fn apply_wal_durability(db: &Connection) -> Result<()> {
    db.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

/// A state.db connection from the pool (see `GxserverDatabase`); it is returned to the pool when dropped.
pub fn open_gxserver_database(paths: &GxserverPaths) -> Result<GxserverDatabase> {
    borrow_gxserver_database(&paths.state_db_file, None)
}

pub fn open_gxserver_database_with_busy_timeout(
    paths: &GxserverPaths,
    busy_timeout: Duration,
) -> Result<GxserverDatabase> {
    /*
    Apply the busy handler before connection PRAGMAs as well as later writes.
    This is intentionally a separate entry point so only operations that are
    designed for concurrent writers opt into lock waiting.
    */
    borrow_gxserver_database(&paths.state_db_file, Some(busy_timeout))
}

pub fn run_gxserver_migrations(db: &mut Connection) -> Result<Vec<String>> {
    db.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
          id TEXT PRIMARY KEY,
          appliedAt TEXT NOT NULL
        );
        "#,
    )?;

    let mut applied = Vec::new();
    for migration in GXSERVER_STORAGE_MIGRATIONS {
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE id = ?1)",
            [migration.id],
            |row| row.get(0),
        )?;
        if exists {
            continue;
        }
        let transaction = db.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (id, appliedAt) VALUES (?1, ?2)",
            (
                migration.id,
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ),
        )?;
        transaction.commit()?;
        applied.push(migration.id.to_string());
    }
    Ok(applied)
}

#[cfg(unix)]
fn set_dir_mode_0700(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_dir_mode_0700(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
