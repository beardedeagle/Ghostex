use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::paths::GxserverPaths;

/// Key in the desktop's `native-sidebar-settings.json`.
pub(crate) const STATUS_SOURCE_SETTING_KEY: &str = "sessionStatusSource";

/// CDXC:SessionStatus 2026-10-14 DECISION:
/// User (Q112): the setting has three values. Detection is today's behaviour (hooks, terminal titles, screen reading). Auto trusts OSC 7501 for a session once that session has reported a record and keeps today's detection for sessions that never reported; since 2026-10-15 (user: "fix that") only while the program that reported is still running, so a session goes back to detection when that program exits. Program status only takes status only from OSC 7501 records. Detection stays the default: Auto would change what every current user sees as soon as Claude Code reports, and Ghostex's detection still knows things the protocol does not carry (subagent fleets, compaction, a Stop hook's "finished" before the agent repaints).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusSource {
    Detection,
    Auto,
    ProgramStatus,
}

impl StatusSource {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("auto") => Self::Auto,
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

/// Tell the module where the desktop keeps its settings. Called once at start.
pub(crate) fn init_status_source(paths: &GxserverPaths) {
    let _ = CONFIG_DIR.set(paths.app_config_dir.clone());
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
