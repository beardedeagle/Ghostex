//! The two Issues Workflow settings that change sessions on their own: `titleSessionsByBranch` and
//! `offerCleanupWhenPrMerges` (Settings > Session Cards). Read from this machine's settings file,
//! falling back to the settings catalog's defaults.
//!
//! SEE-ALSO: packages/settings-catalog/src/data/defaults.rs (the user decision and both defaults),
//! server/src/server/workspaces_switch.rs (a flip runs a work pass, which republishes the rows).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

pub(crate) const TITLE_SESSIONS_BY_BRANCH: &str = "titleSessionsByBranch";
pub(crate) const OFFER_CLEANUP_WHEN_PR_MERGES: &str = "offerCleanupWhenPrMerges";

/// How long one read of the settings file answers: projections ask once per session row.
const FRESH_FOR: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IssuesWorkflowSettings {
    /// Sessions on a work branch are titled by it until renamed.
    pub(crate) title_sessions_by_branch: bool,
    /// A session whose linked PR merged gets the Clean up / Keep offer.
    pub(crate) offer_cleanup_when_pr_merges: bool,
}

impl IssuesWorkflowSettings {
    /// The values in `settings` (the settings file's object), the catalog's defaults for the rest.
    pub(crate) fn from_settings(settings: Option<&Value>) -> Self {
        let read = |key: &str| {
            settings
                .and_then(|settings| settings.get(key))
                .and_then(Value::as_bool)
                .unwrap_or_else(|| {
                    ghostex_settings_catalog::default_value(key)
                        .is_some_and(|value| *value == ghostex_settings_catalog::J::Bool(true))
                })
        };
        Self {
            title_sessions_by_branch: read(TITLE_SESSIONS_BY_BRANCH),
            offer_cleanup_when_pr_merges: read(OFFER_CLEANUP_WHEN_PR_MERGES),
        }
    }
}

fn cache() -> &'static Mutex<Option<(Instant, IssuesWorkflowSettings)>> {
    static CACHE: Mutex<Option<(Instant, IssuesWorkflowSettings)>> = Mutex::new(None);
    &CACHE
}

/// The settings as this machine's settings file says, without the cache.
pub(crate) fn read_issues_workflow_settings(
    paths: &crate::paths::GxserverPaths,
) -> IssuesWorkflowSettings {
    IssuesWorkflowSettings::from_settings(
        crate::session_lifecycle::read_sidebar_settings(paths).as_ref(),
    )
}

/// [`read_issues_workflow_settings`] for this gxserver's own paths, read at most every
/// [`FRESH_FOR`].
pub(crate) fn issues_workflow_settings() -> IssuesWorkflowSettings {
    if let Ok(cache) = cache().lock() {
        if let Some((read_at, settings)) = *cache {
            if read_at.elapsed() < FRESH_FOR {
                return settings;
            }
        }
    }
    let settings = read_issues_workflow_settings(&crate::paths::get_gxserver_paths(None));
    remember_issues_workflow_settings(settings);
    settings
}

/// Records values just read elsewhere (the settings watcher), so every reader agrees.
pub(crate) fn remember_issues_workflow_settings(settings: IssuesWorkflowSettings) {
    if let Ok(mut cache) = cache().lock() {
        *cache = Some((Instant::now(), settings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_title_off_and_cleanup_on() {
        assert_eq!(
            IssuesWorkflowSettings::from_settings(None),
            IssuesWorkflowSettings {
                title_sessions_by_branch: false,
                offer_cleanup_when_pr_merges: true,
            }
        );
        assert_eq!(
            IssuesWorkflowSettings::from_settings(Some(&json!({ "sidebarSessionLinks": "badge" }))),
            IssuesWorkflowSettings::from_settings(None)
        );
    }

    #[test]
    fn saved_values_win_over_the_defaults() {
        let settings = IssuesWorkflowSettings::from_settings(Some(&json!({
            "titleSessionsByBranch": true,
            "offerCleanupWhenPrMerges": false,
        })));
        assert!(settings.title_sessions_by_branch);
        assert!(!settings.offer_cleanup_when_pr_merges);
        // A value that is not a boolean is ignored, like the catalog's other switches.
        let settings = IssuesWorkflowSettings::from_settings(Some(&json!({
            "titleSessionsByBranch": "yes",
        })));
        assert!(!settings.title_sessions_by_branch);
    }
}
