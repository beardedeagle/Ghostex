use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::logging::GxserverLogger;
use crate::paths::GxserverPaths;

/// Key in the desktop's `native-sidebar-settings.json`.
pub(crate) const STATUS_SOURCE_SETTING_KEY: &str = "sessionStatusSource";

/// CDXC:SessionStatus 2026-10-15 DECISION:
/// User (Q120, supersedes the Q112 Auto value): "how about we do something merges both? so we use 7501 to make ghostex itself better". Three values: Detection (Ghostex's own hooks, titles and screen reading only), Combined (detection stays in charge and a program's OSC 7501 report is one more, trusted signal: it adds error, the blocked kind, its message and progress; blocked and done show at once; the more urgent signal wins a disagreement; subagent fleets and compaction still keep a session working), and Program status only (status comes only from the program's reports). Combined is stored as `auto`, the value Auto had, so a user who picked Auto lands on Combined without a migration; `combined` reads the same. Detection stays the default.
/// SEE-ALSO: server/src/session_status/model.rs (the Combined guard), server/src/program_status/ingest.rs (the Combined writes), server/src/program_status/mismatch.rs (Q121's mismatch log).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusSource {
    Detection,
    Combined,
    ProgramStatus,
}

impl StatusSource {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("auto" | "combined") => Self::Combined,
            Some("programStatus") => Self::ProgramStatus,
            _ => Self::Detection,
        }
    }
}

static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();
static CACHE: OnceLock<Mutex<Option<(Instant, StatusSource)>>> = OnceLock::new();
/// The setting is read on every activity write and every presentation pass;
/// a second of staleness is invisible next to a file read per call.
const CACHE_TTL: Duration = Duration::from_secs(1);

static LOGGER: OnceLock<(Arc<GxserverLogger>, String)> = OnceLock::new();

/// Tell the module where the desktop keeps its settings and where the mismatch
/// log goes. Called once at start.
pub(crate) fn init_status_source(
    paths: &GxserverPaths,
    logger: Arc<GxserverLogger>,
    server_id: &str,
) {
    let _ = CONFIG_DIR.set(paths.app_config_dir.clone());
    let _ = LOGGER.set((logger, server_id.to_string()));
}

/// The gxserver logger and server id, once `init_status_source` ran.
pub(crate) fn program_status_logger() -> Option<&'static (Arc<GxserverLogger>, String)> {
    LOGGER.get()
}

/// The current Session status source setting. Detection when gxserver was
/// not initialised (tests) or the file is missing.
pub(crate) fn current_status_source() -> StatusSource {
    let Some(config_dir) = CONFIG_DIR.get() else {
        return StatusSource::Detection;
    };
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(cached) = cache.lock() {
        if let Some((read_at, source)) = *cached {
            if read_at.elapsed() < CACHE_TTL {
                return source;
            }
        }
    }
    let source = read_status_source(config_dir);
    if let Ok(mut cached) = cache.lock() {
        *cached = Some((Instant::now(), source));
    }
    source
}

fn read_status_source(config_dir: &std::path::Path) -> StatusSource {
    let value = std::fs::read_to_string(config_dir.join("native-sidebar-settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    StatusSource::parse(
        value
            .as_ref()
            .and_then(|settings| settings.get(STATUS_SOURCE_SETTING_KEY))
            .and_then(serde_json::Value::as_str),
    )
}
