//! This computer's answer when another device asks to update its Ghostex (`/api/appUpdate*` in gxserver, the `appUpdate` renderer command).
//!
//! The answer always goes through the updater this app already runs (Sparkle on macOS, Velopack on Windows; Linux only announces), so a remote install is the same download, signature check and restart as clicking Update and Restart here, minus the dialogs nobody is in front of. That restart quits the app, never gxserver or the zmx sessions (`flush_gpui_quit_persistence`); the relaunched app replaces gxserver with its own build through `/api/control/stop`, which leaves the sessions and their agents running.
//! SEE-ALSO: server/src/app_update/, apps/desktop/native/macos/GpuiSparkleUpdater.m (`GhostexGpuiSparkleInstallUnattended`), apps/desktop/src/app/os_integration/updater.rs.
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ghostex_gx_core::AppUpdateOp;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

/// Another device asked for an install: the updater installs and restarts without its dialogs.
pub(crate) static REMOTE_INSTALL_REQUESTED: AtomicBool = AtomicBool::new(false);

/// How long a requested check reads as `checking` unless its answer arrives sooner.
const CHECK_PHASE_LIMIT: Duration = Duration::from_secs(20);
/// How long a remote install's failure is reported before the row goes back to the updater's own state.
const ERROR_SHOWN_FOR: Duration = Duration::from_secs(30 * 60);

#[derive(Default)]
struct RemoteControlTrack {
    checking_since: Option<Instant>,
    /// The last remote install's failure and when it happened.
    error: Option<(String, Instant)>,
    /// The remote install's download finished (extracting and installing come next).
    downloaded: bool,
    /// The updater passed the download and is installing.
    installing: bool,
}

static TRACK: Mutex<RemoteControlTrack> = Mutex::new(RemoteControlTrack {
    checking_since: None,
    error: None,
    downloaded: false,
    installing: false,
});

/// The version Sparkle's last probe found, kept beside the app's `update_available` flag.
#[cfg(target_os = "macos")]
pub(crate) static SPARKLE_AVAILABLE_VERSION: Mutex<Option<String>> = Mutex::new(None);

fn with_track<T>(f: impl FnOnce(&mut RemoteControlTrack) -> T) -> T {
    let mut track = TRACK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut track)
}

/// The updater answered a check (an update or none), so a requested check is over.
pub(crate) fn note_update_check_finished() {
    with_track(|track| track.checking_since = None);
}

/// A remote install ended without restarting: say why to whoever asks next.
pub(crate) fn note_remote_install_failed(message: impl Into<String>) {
    if !REMOTE_INSTALL_REQUESTED.swap(false, Ordering::AcqRel) {
        return;
    }
    let message = message.into();
    with_track(|track| {
        track.error = Some((message, Instant::now()));
        track.installing = false;
        track.downloaded = false;
        track.checking_since = None;
    });
}

/// The updater's download started or stopped; a remote install that saw it end is installing.
pub(crate) fn note_update_downloading(downloading: bool) {
    if !downloading && REMOTE_INSTALL_REQUESTED.load(Ordering::Acquire) {
        with_track(|track| track.downloaded = true);
    }
}

/// Sparkle closed its session: an install that never reached Install and Relaunch stopped.
#[cfg(target_os = "macos")]
pub(crate) fn note_remote_install_ended() {
    if with_track(|track| track.installing) {
        return;
    }
    note_remote_install_failed("The update stopped before it installed.");
}

/// The remote install passed its download and is about to restart Ghostex.
pub(crate) fn note_remote_install_installing() {
    with_track(|track| track.installing = true);
}

fn begin_request(checking: bool) {
    with_track(|track| {
        track.error = None;
        track.installing = false;
        track.downloaded = false;
        track.checking_since = checking.then(Instant::now);
    });
}

impl GhostexGpuiApp {
    pub(crate) fn answer_app_update_renderer_command(
        &mut self,
        op: AppUpdateOp,
        cx: &mut gpui::Context<Self>,
    ) -> Value {
        let accepted = match op {
            AppUpdateOp::Status => None,
            AppUpdateOp::Check => Some(self.start_remote_update_check(cx)),
            AppUpdateOp::Install => Some(self.start_remote_update_install(cx)),
        };
        let mut report = self.app_update_report();
        if let (Some(result), Some(object)) = (accepted, report.as_object_mut()) {
            object.insert("accepted".to_string(), json!(result.is_ok()));
            if let Err(message) = result {
                object.insert("message".to_string(), json!(message));
            }
        }
        json!({ "ok": true, "appUpdate": report })
    }

    /// `{ platform, appVersion, supported, unsupportedReason, phase, updateAvailable, availableVersion, progress, message }`.
    fn app_update_report(&self) -> Value {
        let (checking, error, installing) = with_track(|track| {
            if track
                .checking_since
                .is_some_and(|since| since.elapsed() > CHECK_PHASE_LIMIT)
            {
                track.checking_since = None;
            }
            if track
                .error
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() > ERROR_SHOWN_FOR)
            {
                track.error = None;
            }
            (
                track.checking_since.is_some(),
                track.error.as_ref().map(|(message, _)| message.clone()),
                track.installing || track.downloaded,
            )
        });
        let (supported, unsupported_reason) = self.app_update_support();
        let available_version = self.app_update_available_version();
        let available = self.update_available || available_version.is_some();
        let phase = if installing {
            "installing"
        } else if self.update_downloading {
            "downloading"
        } else if REMOTE_INSTALL_REQUESTED.load(Ordering::Acquire) || checking {
            "checking"
        } else if error.is_some() {
            "failed"
        } else if available {
            "available"
        } else if supported {
            "upToDate"
        } else {
            "idle"
        };
        json!({
            "platform": std::env::consts::OS,
            "appVersion": env!("GHOSTEX_BUILD_MARKETING_VERSION"),
            "supported": supported,
            "unsupportedReason": unsupported_reason,
            "phase": phase,
            "updateAvailable": available,
            "availableVersion": available_version,
            "progress": self.update_downloading.then_some(self.update_download_progress).flatten(),
            "message": error,
        })
    }

    #[cfg(target_os = "macos")]
    fn app_update_support(&self) -> (bool, Option<&'static str>) {
        if self.updater_started {
            (true, None)
        } else {
            (
                false,
                Some("This Ghostex build was packaged without its updater."),
            )
        }
    }

    #[cfg(target_os = "windows")]
    fn app_update_support(&self) -> (bool, Option<&'static str>) {
        if self.updater_started {
            (true, None)
        } else {
            (
                false,
                Some(
                    "This Ghostex build was not installed with the Windows installer, so it cannot update itself.",
                ),
            )
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn app_update_support(&self) -> (bool, Option<&'static str>) {
        (
            false,
            Some(
                "Linux updates come from the package you installed Ghostex with; install the new version there.",
            ),
        )
    }

    #[cfg(target_os = "macos")]
    fn app_update_available_version(&self) -> Option<String> {
        if !self.update_available {
            return None;
        }
        SPARKLE_AVAILABLE_VERSION
            .lock()
            .ok()
            .and_then(|version| version.clone())
    }

    #[cfg(target_os = "windows")]
    fn app_update_available_version(&self) -> Option<String> {
        self.windows_update
            .as_ref()
            .map(|update| update.version().to_string())
    }

    #[cfg(target_os = "linux")]
    fn app_update_available_version(&self) -> Option<String> {
        self.linux_update
            .as_ref()
            .map(|update| update.version().to_string())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    fn app_update_available_version(&self) -> Option<String> {
        None
    }

    fn start_remote_update_check(&mut self, cx: &mut gpui::Context<Self>) -> Result<(), String> {
        let (supported, _) = self.app_update_support();
        if !supported && !cfg!(target_os = "linux") {
            return Err("This Ghostex build cannot check for updates.".to_string());
        }
        begin_request(true);
        if !self.forward_to_lead_window(cx, |app, _window, cx| app.run_remote_update_check(cx)) {
            self.run_remote_update_check(cx);
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn run_remote_update_check(&mut self, _cx: &mut gpui::Context<Self>) {
        if self.updater_started {
            unsafe { crate::app::helpers::GhostexGpuiSparkleProbeForUpdateInformation() };
        }
    }

    #[cfg(target_os = "windows")]
    fn run_remote_update_check(&mut self, cx: &mut gpui::Context<Self>) {
        self.begin_windows_update_check(false, cx);
    }

    #[cfg(target_os = "linux")]
    fn run_remote_update_check(&mut self, cx: &mut gpui::Context<Self>) {
        if self.updater_started {
            self.begin_linux_update_check(false, cx);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    fn run_remote_update_check(&mut self, _cx: &mut gpui::Context<Self>) {}

    fn start_remote_update_install(&mut self, cx: &mut gpui::Context<Self>) -> Result<(), String> {
        let (supported, unsupported_reason) = self.app_update_support();
        if !supported {
            return Err(unsupported_reason
                .unwrap_or("This Ghostex build cannot update itself.")
                .to_string());
        }
        if self.update_downloading && !REMOTE_INSTALL_REQUESTED.load(Ordering::Acquire) {
            return Err(
                "An update is already downloading on this computer; finish it there.".to_string(),
            );
        }
        if REMOTE_INSTALL_REQUESTED.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        begin_request(false);
        if !self.forward_to_lead_window(cx, |app, _window, cx| app.run_remote_update_install(cx)) {
            self.run_remote_update_install(cx);
        }
        Ok(())
    }

    /// Sparkle's standard flow with the update-found and ready-to-install answers given for the user (`GhostexGpuiSparkleInstallUnattended`).
    #[cfg(target_os = "macos")]
    fn run_remote_update_install(&mut self, _cx: &mut gpui::Context<Self>) {
        match unsafe { crate::app::helpers::GhostexGpuiSparkleInstallUnattended() } {
            1 => {}
            2 => note_remote_install_failed(
                "An update window is open on this computer; finish or close it there and try again.",
            ),
            _ => note_remote_install_failed("Sparkle could not start the update on this computer."),
        }
    }

    /// The Windows dialog's steps without the dialog: check if needed, download, then restart and apply (updater.rs reads `REMOTE_INSTALL_REQUESTED` between them).
    #[cfg(target_os = "windows")]
    fn run_remote_update_install(&mut self, cx: &mut gpui::Context<Self>) {
        if self.windows_ready_update.is_some() {
            self.restart_for_remote_windows_update(cx);
        } else if self.windows_update.is_some() {
            self.download_windows_update(cx);
        } else {
            self.begin_windows_update_check(false, cx);
        }
    }

    /// The restart waits a moment so the renderer command's answer reaches gxserver first.
    #[cfg(target_os = "windows")]
    pub(crate) fn restart_for_remote_windows_update(&mut self, cx: &mut gpui::Context<Self>) {
        note_remote_install_installing();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(800))
                .await;
            let _ = this.update(cx, |this, cx| this.restart_and_apply_windows_update(cx));
        })
        .detach();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn run_remote_update_install(&mut self, _cx: &mut gpui::Context<Self>) {
        note_remote_install_failed(
            "Linux updates come from the package you installed Ghostex with.",
        );
    }
}
