//! Whether Connectors is on: it is experimental, so it follows Enable Experimental Features
//! (`showBetaFeatures`). The CLI hides and refuses its verbs, the routes refuse, and the Executor's
//! idle-stop thread starts only once a Connectors route is actually served.

use serde_json::Value;

/// The setting that turns Connectors on.
pub(crate) const CONNECTORS_SETTING_KEY: &str = "showBetaFeatures";

/// What the CLI and the routes answer while Connectors is off.
pub(crate) const CONNECTORS_OFF_MESSAGE: &str = "Connectors is experimental. Turn it on in Settings › General › Advanced › Experimental › Enable Experimental Features, or run: ghostex settings set showBetaFeatures true";

/// CDXC:Connectors 2026-10-15 DECISION:
/// User: Connectors should be on main "but should be experimental so not shown without that toggle enabled" (Enable Experimental Features, `showBetaFeatures`). While it is off, `ghostex connectors` is left out of `ghostex --help` and refuses, the Connectors routes refuse, and nothing Connectors-related runs in the background. A future Connectors page in the desktop or phone app must follow this same gate.
pub(crate) fn connectors_enabled_in(settings: Option<&Value>) -> bool {
    settings
        .and_then(|settings| settings.get(CONNECTORS_SETTING_KEY))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Whether Connectors is on, read fresh from this gxserver's settings file, so flipping the switch
/// takes effect without a restart.
pub(crate) fn connectors_enabled(paths: &crate::paths::GxserverPaths) -> bool {
    connectors_enabled_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}
