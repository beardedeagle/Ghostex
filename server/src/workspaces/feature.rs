//! Whether the Workspaces built-in extension is on for this machine (`workspacesHidden` on
//! Settings > Extensions). While it is off gxserver behaves as if workspaces did not exist: the
//! workspaces document and the projects' workspace ids are not published (so every client shows
//! every project, as a daemon without workspaces does), the team subscriptions stop and a team's
//! tracker choice does not apply, added projects stay where they are stored, and launches get no
//! workspace Claude account. Issues Workflow stays on (CDXC:WorkMode 2026-10-24 in
//! server/src/work_mode/project.rs). Nothing saved is touched.
//!
//! CDXC:Workspaces 2026-10-09 SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs (the user decision), server/src/server/workspaces_switch.rs (republishing when it flips).

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long one read of the settings file answers: projections ask once per project row.
const FRESH_FOR: Duration = Duration::from_secs(2);

fn cache() -> &'static Mutex<Option<(Instant, bool)>> {
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    &CACHE
}

/// Whether Workspaces is on, as this machine's settings file says (the defaults when it has
/// none, which is off).
pub(crate) fn read_workspaces_feature_enabled(paths: &crate::paths::GxserverPaths) -> bool {
    ghostex_settings_catalog::built_in_extensions::enabled_in_value(
        crate::session_lifecycle::read_sidebar_settings(paths).as_ref(),
        ghostex_settings_catalog::built_in_extensions::WORKSPACES,
    )
}

/// [`read_workspaces_feature_enabled`] for this gxserver's own paths, read at most every
/// [`FRESH_FOR`].
pub(crate) fn workspaces_feature_enabled() -> bool {
    if let Ok(cache) = cache().lock() {
        if let Some((read_at, enabled)) = *cache {
            if read_at.elapsed() < FRESH_FOR {
                return enabled;
            }
        }
    }
    let enabled = read_workspaces_feature_enabled(&crate::paths::get_gxserver_paths(None));
    remember_workspaces_feature_enabled(enabled);
    enabled
}

/// Records a value just read elsewhere (the switch watcher), so every reader agrees with what was
/// published.
pub(crate) fn remember_workspaces_feature_enabled(enabled: bool) {
    if let Ok(mut cache) = cache().lock() {
        *cache = Some((Instant::now(), enabled));
    }
}
