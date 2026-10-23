//! A web runtime change the user asked for while Chromium was already running in this process:
//! Uninstall (macOS and Windows) and Reinstall (Windows, which keeps a loaded DLL locked). The
//! change is written next to the installed components and carried out at the next launch, in
//! `main` before anything can load the runtime.
//!
//! CDXC:CefRuntime 2026-10-23 DECISION:
//! User: the web runtime now starts at every launch (CDXC:CefRuntime 2026-10-23 in app/cef_deferred_startup.rs), so Uninstall could no longer run before Chromium started; asked how it should work, the user picked "Remove it at the next launch": Uninstall while the runtime runs marks it for removal and the next launch deletes it before anything can start it, and a Windows Reinstall swaps it at the next launch the same way. Plugins says so and offers to undo the mark; Uninstall before the runtime started still removes it at once. Linux is left out: its launcher installs the runtime before every start and the app links it at load, so a removal could not stick there.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The file in the component store's folder that holds the pending change.
const PENDING_CHANGE_FILE: &str = "web-runtime-pending-change";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingWebRuntimeChange {
    Uninstall,
    Reinstall,
}

impl PendingWebRuntimeChange {
    fn as_str(self) -> &'static str {
        match self {
            Self::Uninstall => "uninstall",
            Self::Reinstall => "reinstall",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "uninstall" => Some(Self::Uninstall),
            "reinstall" => Some(Self::Reinstall),
            _ => None,
        }
    }

    /// What Plugins calls it.
    pub(crate) fn wire(self) -> &'static str {
        self.as_str()
    }
}

/// Whether this platform defers a change to the next launch instead of refusing it.
pub(crate) const PENDING_CHANGES_SUPPORTED: bool =
    cfg!(any(target_os = "macos", target_os = "windows"));

/// Set at launch when a pending Reinstall removed the runtime, so the app downloads it again once
/// its first window is up (`take_reinstall_after_launch`).
static REINSTALL_AFTER_LAUNCH: AtomicBool = AtomicBool::new(false);

/// What the launch did with a pending change.
#[cfg_attr(target_os = "linux", allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LaunchChange {
    /// Nothing was pending.
    None,
    /// The runtime was removed; a Reinstall downloads it again (`reinstall`).
    Removed { reinstall: bool },
    /// Removing it failed. The mark stays for the next launch and the runtime is not started in
    /// this one, because the user asked for it to go.
    RemovalFailed(String),
}

fn pending_path(root: &Path) -> PathBuf {
    root.join(PENDING_CHANGE_FILE)
}

fn read_pending_in(root: &Path) -> Option<PendingWebRuntimeChange> {
    PendingWebRuntimeChange::parse(&fs::read_to_string(pending_path(root)).ok()?)
}

fn mark_pending_in(root: &Path, change: PendingWebRuntimeChange) -> Result<(), String> {
    fs::create_dir_all(root)
        .and_then(|()| fs::write(pending_path(root), change.as_str()))
        .map_err(|error| format!("The change could not be saved for the next launch: {error}"))
}

fn clear_pending_in(root: &Path) -> Result<(), String> {
    match fs::remove_file(pending_path(root)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("The pending change could not be undone: {error}")),
    }
}

/// Carries out the change pending under `root` with `remove` (which removes the installed
/// runtime and says whether there was one to remove).
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn apply_pending_in(root: &Path, remove: impl FnOnce() -> Result<bool, String>) -> LaunchChange {
    let Some(change) = read_pending_in(root) else {
        return LaunchChange::None;
    };
    if let Err(error) = remove() {
        return LaunchChange::RemovalFailed(error);
    }
    if let Err(error) = clear_pending_in(root) {
        return LaunchChange::RemovalFailed(error);
    }
    LaunchChange::Removed {
        reinstall: change == PendingWebRuntimeChange::Reinstall,
    }
}

fn store_root() -> Result<PathBuf, String> {
    crate::component_store::component_store_root()
}

/// The change waiting for the next launch, if any.
pub(crate) fn pending_web_runtime_change() -> Option<PendingWebRuntimeChange> {
    read_pending_in(&store_root().ok()?)
}

pub(crate) fn mark_pending_web_runtime_change(
    change: PendingWebRuntimeChange,
) -> Result<(), String> {
    mark_pending_in(&store_root()?, change)
}

/// Plugins' Undo, and any Install or Reinstall the user starts, which wants the runtime kept.
pub(crate) fn cancel_pending_web_runtime_change() -> Result<(), String> {
    clear_pending_in(&store_root()?)
}

/// Called from `main` before the runtime is looked for. Returns whether the runtime may be used
/// in this run: not when a removal the user asked for failed.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn apply_pending_web_runtime_change_at_launch() -> bool {
    let Ok(root) = store_root() else {
        return true;
    };
    // A runtime carried next to the app is never marked (Uninstall refuses it); a mark left from
    // an installed build is dropped.
    if crate::cef_component_window::cef_runtime_is_bundled() {
        let _ = clear_pending_in(&root);
        return true;
    }
    match apply_pending_in(&root, crate::cef_component_window::uninstall_cef_component) {
        LaunchChange::None => true,
        LaunchChange::Removed { reinstall } => {
            REINSTALL_AFTER_LAUNCH.store(reinstall, Ordering::Relaxed);
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::HostLifecycle,
                "gpui.webRuntime.pendingChangeApplied",
                serde_json::json!({ "reinstall": reinstall }),
            );
            true
        }
        LaunchChange::RemovalFailed(error) => {
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::CrashReports,
                "gpui.webRuntime.pendingRemovalFailed",
                serde_json::json!({ "error": error }),
            );
            false
        }
    }
}

/// Whether the launch removed the runtime for a Reinstall that should download it again now.
pub(crate) fn take_reinstall_after_launch() -> bool {
    REINSTALL_AFTER_LAUNCH.swap(false, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ghostex-web-runtime-pending-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn a_mark_is_read_back_and_undone() {
        let root = scratch("mark");
        assert_eq!(read_pending_in(&root), None);
        mark_pending_in(&root, PendingWebRuntimeChange::Uninstall).unwrap();
        assert_eq!(
            read_pending_in(&root),
            Some(PendingWebRuntimeChange::Uninstall)
        );
        mark_pending_in(&root, PendingWebRuntimeChange::Reinstall).unwrap();
        assert_eq!(
            read_pending_in(&root),
            Some(PendingWebRuntimeChange::Reinstall)
        );
        clear_pending_in(&root).unwrap();
        assert_eq!(read_pending_in(&root), None);
        // Undoing with nothing pending is fine.
        clear_pending_in(&root).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn launch_without_a_mark_removes_nothing() {
        let root = scratch("none");
        let change = apply_pending_in(&root, || panic!("nothing is pending"));
        assert_eq!(change, LaunchChange::None);
    }

    #[test]
    fn launch_removes_a_runtime_marked_for_removal_once() {
        let root = scratch("uninstall");
        mark_pending_in(&root, PendingWebRuntimeChange::Uninstall).unwrap();
        let mut removed = 0;
        let change = apply_pending_in(&root, || {
            removed += 1;
            Ok(true)
        });
        assert_eq!(change, LaunchChange::Removed { reinstall: false });
        assert_eq!(removed, 1);
        assert_eq!(read_pending_in(&root), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn launch_removes_a_runtime_marked_for_reinstall_and_asks_for_the_download() {
        let root = scratch("reinstall");
        mark_pending_in(&root, PendingWebRuntimeChange::Reinstall).unwrap();
        let change = apply_pending_in(&root, || Ok(true));
        assert_eq!(change, LaunchChange::Removed { reinstall: true });
        assert_eq!(read_pending_in(&root), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_removal_keeps_the_mark_for_the_next_launch() {
        let root = scratch("failed");
        mark_pending_in(&root, PendingWebRuntimeChange::Uninstall).unwrap();
        let change = apply_pending_in(&root, || Err("in use".to_string()));
        assert_eq!(change, LaunchChange::RemovalFailed("in use".to_string()));
        assert_eq!(
            read_pending_in(&root),
            Some(PendingWebRuntimeChange::Uninstall)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreadable_mark_is_ignored() {
        let root = scratch("garbage");
        fs::create_dir_all(&root).unwrap();
        fs::write(pending_path(&root), "something else").unwrap();
        assert_eq!(read_pending_in(&root), None);
        let _ = fs::remove_dir_all(&root);
    }
}
