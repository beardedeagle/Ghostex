//! The Trycua install, update, reinstall or uninstall job the desktop app runs in the background
//! (install_job.rs runs it), and the fields it adds to the `ghostexCliStatus` payload.

use crate::app::helpers::*;

pub(crate) static CUA_DRIVER_JOB: GpuiInstallJob =
    GpuiInstallJob::new("Fast Computer & Browser Use");

/// Adds the Trycua job, install plan and /Applications check to a `ghostexCliStatus` payload.
pub(crate) fn gpui_decorate_cua_driver_status(payload: &mut serde_json::Value) {
    payload["cuaDriverJob"] = CUA_DRIVER_JOB.json();
    payload["cuaDriverInstallPlan"] = serde_json::json!(gpui_cua_driver_install_plan());
    payload["cuaDriverApplicationsBlockedReason"] = gpui_cua_driver_applications_blocked_reason()
        .map_or(serde_json::Value::Null, |reason| serde_json::json!(reason));
}

/// Tooltip for Install and Reinstall: exactly what one click runs.
pub(crate) fn gpui_cua_driver_install_plan() -> String {
    if cfg!(target_os = "windows") {
        "Runs Fast Computer & Browser Use's official installer from cua.ai (irm https://cua.ai/driver/install.ps1 | iex) in the background. Windows shows one administrator prompt to let Fast Computer & Browser Use start with Windows; you can decline it.".to_string()
    } else if cfg!(target_os = "macos") {
        "Runs Fast Computer & Browser Use's official installer from cua.ai (curl -fsSL https://cua.ai/driver/install.sh | bash) in the background; it puts CuaDriver.app in /Applications and starts it. No password needed; macOS then asks you to allow Accessibility and Screen Recording.".to_string()
    } else {
        "Runs Fast Computer & Browser Use's official installer from cua.ai (curl -fsSL https://cua.ai/driver/install.sh | bash) in the background. No password needed.".to_string()
    }
}

/// macOS: why Install cannot run, when this account cannot write /Applications.
pub(crate) fn gpui_cua_driver_applications_blocked_reason() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let path = std::ffi::CString::new("/Applications").ok()?;
        // SAFETY: `path` is a valid NUL-terminated C string for the duration of the call.
        let writable = unsafe { libc::access(path.as_ptr(), libc::W_OK) } == 0;
        if !writable {
            return Some("Fast Computer & Browser Use's installer copies CuaDriver.app into /Applications, which this account can't change. Sign in as an administrator, or ask one to install Fast Computer & Browser Use.".to_string());
        }
    }
    None
}
