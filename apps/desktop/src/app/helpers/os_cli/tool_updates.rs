//! What Fast Computer & Browser Use and SpaceO share about updates: the toast for Settings'
//! explicit "check for updates" click, read from the refreshed `ghostexCliStatus`.

/// CDXC:ManagedTools 2026-10-14 DECISION:
/// User: "we need an updating system for SpaceO like we have for cua-driver." Both tools report `{prefix}Version`, `{prefix}LatestVersion` and `{prefix}UpdateAvailable` in the status, share one Update / Up to date / Check button (integrations/updates.rs) and this toast, and update through the same background install job; SpaceO's update restarts its daemon with the operator restart that waits for agents.
pub(crate) fn gpui_tool_update_check_toast(
    status: &serde_json::Value,
    name: &str,
    prefix: &str,
) -> (&'static str, String, String) {
    let text = |key: &str| {
        status
            .get(format!("{prefix}{key}"))
            .and_then(serde_json::Value::as_str)
    };
    let installed = text("Version");
    let latest = text("LatestVersion");
    match status
        .get(format!("{prefix}UpdateAvailable"))
        .and_then(serde_json::Value::as_bool)
    {
        Some(true) => (
            "info",
            format!("{name} update available"),
            match (installed, latest) {
                (Some(installed), Some(latest)) => {
                    format!("Version {latest} is available; {installed} is installed.")
                }
                _ => format!("A newer {name} release is available."),
            },
        ),
        Some(false) => (
            "success",
            format!("{name} is up to date"),
            match installed.or(latest) {
                Some(version) => format!("Version {version} is the latest release."),
                None => format!("You have the latest {name} release."),
            },
        ),
        None => (
            "warning",
            format!("Couldn't check for {name} updates"),
            "The update check did not answer. Check your connection and try again.".to_string(),
        ),
    }
}
