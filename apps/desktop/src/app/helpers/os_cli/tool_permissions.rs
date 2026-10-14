//! The Accessibility and Screen Recording grants of the desktop-control tools (SpaceO and Fast
//! Computer & Browser Use) for Settings > Integrations: each grant's state, the Grant button's
//! real system request from the tool's own identity, SpaceO's restart once a grant lands, and
//! the quick permission-only refresh Settings polls while it is open.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::app::helpers::*;

/// CDXC:ManagedTools 2026-10-14 DECISION:
/// User: "We should always show the status of each permission after it's done." One row per permission with a live status that stays visible after the grant (Granted, Not granted, Needs restart); Grant makes the tool request its own permission (so it is listed in System Settings) and opens that Privacy pane; Settings re-checks from a fresh process, since a running process never sees a new grant, until the grant lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GpuiToolPermissionState {
    Granted,
    NotGranted,
    /// macOS has the grant, but the tool's long-running service started before it and still
    /// runs without it until it restarts.
    NeedsRestart,
    #[default]
    Unknown,
}

impl GpuiToolPermissionState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::NotGranted => "notGranted",
            Self::NeedsRestart => "needsRestart",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn from_granted(granted: Option<bool>) -> Self {
        match granted {
            Some(true) => Self::Granted,
            Some(false) => Self::NotGranted,
            None => Self::Unknown,
        }
    }
}

/// What `spaceo doctor --json`, run as SpaceO's own identity, says about its grants and daemon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSpaceoPermissionReport {
    pub(crate) daemon_running: bool,
    /// The app macOS attributes the daemon's grants to, as the daemon reports it.
    pub(crate) permission_app: Option<String>,
    pub(crate) accessibility: GpuiToolPermissionState,
    pub(crate) screen_recording: GpuiToolPermissionState,
}

impl GpuiSpaceoPermissionReport {
    pub(crate) fn detail(&self) -> String {
        use GpuiToolPermissionState::*;
        match (self.accessibility, self.screen_recording) {
            (Granted, Granted) => "SpaceO has Accessibility and Screen Recording.".to_string(),
            (NeedsRestart, _) | (_, NeedsRestart) => {
                "SpaceO needs a restart to use the permissions it was granted.".to_string()
            }
            (Unknown, Unknown) => "SpaceO's permissions could not be checked.".to_string(),
            _ => "SpaceO permissions need attention.".to_string(),
        }
    }
}

/// CDXC:ManagedTools 2026-10-14 WHY:
/// A process reads its Screen Recording grant once, so the daemon keeps reporting a grant made after it started as missing; a fresh `spaceo` process with SpaceO's identity (`capabilities`) shows what macOS has now, and the daemon's own answer (`daemon`) shows what agents get. Granted for SpaceO but not for the daemon is Needs restart.
pub(crate) fn gpui_spaceo_permission_report(path: &Path) -> GpuiSpaceoPermissionReport {
    let Some(doctor) = gpui_run_disclaimed_command_with_captured_output_timeout(
        path,
        &["doctor", "--json"],
        Duration::from_secs(10),
        256 * 1024,
    )
    .ok()
    // doctor exits 1 whenever it found something to fix; its JSON is still complete.
    .and_then(|output| gpui_spaceo_json_object(&output.stdout)) else {
        return GpuiSpaceoPermissionReport::default();
    };
    let identity = |name: &str| {
        doctor
            .get("capabilities")?
            .as_array()?
            .iter()
            .find(|item| item.get("name").and_then(serde_json::Value::as_str) == Some(name))?
            .get("available")?
            .as_bool()
    };
    let daemon = doctor.get("daemon");
    let daemon_field = |key: &str| daemon.and_then(|daemon| daemon.get(key));
    let daemon_running = daemon_field("running").and_then(serde_json::Value::as_bool) == Some(true);
    let state = |identity: Option<bool>, daemon_key: &str| match identity {
        Some(true)
            if daemon_running
                && daemon_field(daemon_key).and_then(serde_json::Value::as_bool) == Some(false) =>
        {
            GpuiToolPermissionState::NeedsRestart
        }
        granted => GpuiToolPermissionState::from_granted(granted),
    };
    GpuiSpaceoPermissionReport {
        daemon_running,
        permission_app: daemon_field("responsibleProcess")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|app| !app.is_empty())
            .map(str::to_string),
        accessibility: state(identity("accessibility"), "accessibilityGranted"),
        screen_recording: state(identity("screen-recording"), "screenRecordingGranted"),
    }
}

/// The Grant button for SpaceO: `spaceo setup` as SpaceO's own identity asks macOS for every
/// missing grant (`CGRequestScreenCaptureAccess`, `AXIsProcessTrustedWithOptions` with the prompt),
/// which adds `spaceo` to both Privacy lists. `--no-self-test` keeps it from starting a daemon.
pub(crate) fn gpui_spaceo_request_permissions(path: &Path) -> Result<(), String> {
    gpui_run_disclaimed_command_with_captured_output_timeout(
        path,
        &["setup", "--no-self-test", "--yes", "--json"],
        Duration::from_secs(30),
        64 * 1024,
    )
    .map(|_| ())
}

static SPACEO_RESTARTING: AtomicBool = AtomicBool::new(false);

/// True while a SpaceO restart started here is waiting for agents' sessions to finish.
pub(crate) fn gpui_spaceo_restarting() -> bool {
    SPACEO_RESTARTING.load(Ordering::SeqCst)
}

/// Restarts SpaceO's daemon so it picks up new grants. `--operator` without `--now` drains: the
/// daemon refuses new sessions, waits for running ones to end, and launchd starts it again as
/// the LaunchAgent, so the restart never ends an agent's session. `None` when one is running.
pub(crate) fn gpui_spaceo_restart_daemon(path: &Path) -> Option<Result<(), String>> {
    if SPACEO_RESTARTING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = gpui_run_command_with_captured_output_timeout(
        path,
        &["daemon", "restart", "--operator"],
        Duration::from_secs(30 * 60),
        16 * 1024,
    )
    .and_then(|output| {
        if output.success {
            Ok(())
        } else {
            Err(output.combined_text().trim().to_string())
        }
    });
    SPACEO_RESTARTING.store(false, Ordering::SeqCst);
    Some(result)
}

/// The Grant button for Fast Computer & Browser Use: `cua-driver permissions grant` opens
/// CuaDriver.app through LaunchServices, so its requests and the System Settings entries are
/// CuaDriver's (`com.trycua.driver`), never Ghostex's; the app walks the user through each pane.
pub(crate) fn gpui_cua_driver_request_permissions(path: &Path) -> Result<(), String> {
    gpui_run_command_with_captured_output_timeout(
        path,
        &["permissions", "grant"],
        Duration::from_secs(10 * 60),
        64 * 1024,
    )
    .map(|_| ())
}

/// A `ghostexCliStatus` update with only the grant fields, cheap enough for Settings to ask for
/// every few seconds while a grant is pending; the Settings store merges it (`partial`).
pub(crate) fn gpui_tool_permission_status_message() -> serde_json::Value {
    let mut payload = serde_json::json!({ "type": "ghostexCliStatus", "partial": true });
    if let Some(path) = gpui_spaceo_executable_path() {
        let report = gpui_spaceo_permission_report(&path);
        payload["spaceoDaemonRunning"] = serde_json::json!(report.daemon_running);
        payload["spaceoPermissionApp"] = serde_json::json!(report.permission_app);
        payload["spaceoAccessibilityPermission"] = serde_json::json!(report.accessibility.as_str());
        payload["spaceoScreenRecordingPermission"] =
            serde_json::json!(report.screen_recording.as_str());
        payload["spaceoRestarting"] = serde_json::json!(gpui_spaceo_restarting());
    }
    let cua_driver_path = gpui_cua_driver_executable_path();
    if cua_driver_path.is_some() {
        let cua_app_installed = gpui_is_dir(Path::new("/Applications/CuaDriver.app"));
        let cua = gpui_cua_driver_permission_status(cua_driver_path.as_deref(), cua_app_installed);
        payload["cuaDriverAccessibilityPermissionGranted"] =
            serde_json::json!(cua.accessibility_granted);
        payload["cuaDriverScreenRecordingPermissionGranted"] =
            serde_json::json!(cua.screen_recording_granted);
        payload["cuaDriverPermissionDetail"] = serde_json::json!(cua.detail);
    }
    payload
}

/// How many sessions are running in SpaceO right now (`session list --operator`, read-only);
/// `None` when no daemon answers. SpaceO restarts by itself only once this reaches 0.
pub(crate) fn gpui_spaceo_live_sessions(path: &Path) -> Option<usize> {
    let output = gpui_run_command_with_captured_output_timeout(
        path,
        &["session", "list", "--operator", "--json"],
        Duration::from_secs(5),
        256 * 1024,
    )
    .ok()
    .filter(|output| output.success)?;
    gpui_spaceo_json_object(&output.stdout)?
        .get("sessions")?
        .as_array()
        .map(Vec::len)
}
