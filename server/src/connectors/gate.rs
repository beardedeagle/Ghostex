//! Whether Connectors is on: it is experimental, so it follows Enable Experimental Features
//! (`showBetaFeatures`), and under that its built-in extension switch (`connectorsHidden`, on by
//! default; CDXC:Extensions 2026-10-19 in packages/settings-catalog/src/data/official_extensions.rs).
//! The CLI hides and refuses its verbs, the routes refuse, and the Executor's idle-stop thread
//! starts only once a Connectors route is actually served.

use ghostex_settings_catalog::built_in_extensions::{self, CONNECTORS};
use serde_json::Value;

/// The setting that turns Connectors on.
pub(crate) const CONNECTORS_SETTING_KEY: &str = "showBetaFeatures";

/// What the CLI and the routes answer while Enable Experimental Features is off.
const CONNECTORS_EXPERIMENTAL_MESSAGE: &str = "Connectors is experimental. Turn it on in Settings › General › Advanced › Experimental › Enable Experimental Features, or run: ghostex settings set showBetaFeatures true";

fn experimental_on(settings: Option<&Value>) -> bool {
    settings
        .and_then(|settings| settings.get(CONNECTORS_SETTING_KEY))
        .and_then(Value::as_bool)
        == Some(true)
}

/// What the CLI and the routes answer while Connectors is off: how to open the experimental gate,
/// or, with the gate open, how to turn its switch back on.
pub(crate) fn connectors_off_message_in(settings: Option<&Value>) -> String {
    if !experimental_on(settings) {
        return CONNECTORS_EXPERIMENTAL_MESSAGE.to_string();
    }
    format!(
        "{} From a terminal: ghostex settings set connectorsHidden false",
        built_in_extensions::turned_off_message(CONNECTORS)
    )
}

/// [`connectors_off_message_in`] for this gxserver's settings file.
pub(crate) fn connectors_off_message(paths: &crate::paths::GxserverPaths) -> String {
    connectors_off_message_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}

/// CDXC:Connectors 2026-10-15 DECISION:
/// User: Connectors should be on main "but should be experimental so not shown without that toggle enabled" (Enable Experimental Features, `showBetaFeatures`). While it is off, `ghostex connectors` is left out of `ghostex --help` and refuses, the Connectors routes refuse, and nothing Connectors-related runs in the background. A future Connectors page in the desktop or phone app must follow this same gate.
pub(crate) fn connectors_enabled_in(settings: Option<&Value>) -> bool {
    experimental_on(settings) && built_in_extensions::enabled_in_value(settings, CONNECTORS)
}

/// Whether Connectors is on, read fresh from this gxserver's settings file, so flipping the switch
/// takes effect without a restart.
pub(crate) fn connectors_enabled(paths: &crate::paths::GxserverPaths) -> bool {
    connectors_enabled_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}
