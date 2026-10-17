//! One Ghostex per data folder on Windows.
//!
//! CDXC:PlatformSupport 2026-09-28 WHY:
//! Chromium's profile lock used to make a second Ghostex hand its launch to the running one (`on_already_running_app_relaunch` in cef/shell/lifecycle.rs), because CEF started with the app. CEF now starts only when a web view is shown (CDXC:CefRuntime 2026-09-28 in app/helpers/web_runtime.rs), so opening Ghostex again from the desktop icon, Start menu or taskbar would start a second full app on the same sessions. A named mutex per CEF profile folder keeps the old behaviour without CEF: the second process brings the running Ghostex forward and exits.

use std::os::windows::ffi::OsStrExt as _;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, RECT};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

/// True when another Ghostex already runs on this data folder; it has been brought forward and
/// this process should exit.
pub(crate) fn hand_off_to_running_instance() -> bool {
    let profile = std::env::var_os("GHOSTEX_GPUI_CEF_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::shared_settings::ghostex_storage_paths().cef_cache_dir());
    // FNV-1a over the lower-cased folder: mutex names cannot contain backslashes.
    let hash = profile
        .to_string_lossy()
        .to_lowercase()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let name = format!("Local\\Ghostex-{hash:016x}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call. The handle is kept
    // open for the life of the process on purpose: it is the lock.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() || unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return false;
    }
    activate_running_instance();
    true
}

/// Brings the other Ghostex's main window forward: the largest visible unowned top-level window of
/// another process running this same executable file. The full path, not just `Ghostex.exe`, so a
/// portable copy relaunched beside an installed Ghostex raises its own window (ghostex_paths::portable).
fn activate_running_instance() {
    struct Search {
        executable: Vec<u16>,
        best: (HWND, i64),
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `Search` below, alive for the whole EnumWindows call.
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == std::process::id()
            || unsafe { IsWindowVisible(hwnd) } == 0
            || !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null()
            || process_executable_path(pid).as_deref() != Some(search.executable.as_slice())
        {
            return 1;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe { GetWindowRect(hwnd, &mut rect) };
        let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
        if area > search.best.1 {
            search.best = (hwnd, area);
        }
        1
    }
    let Some(executable) = std::env::current_exe().ok().map(|path| path_key(&path)) else {
        return;
    };
    let mut search = Search {
        executable,
        best: (std::ptr::null_mut(), 0),
    };
    // SAFETY: `visit` only reads windows and writes through the pointer to `search`.
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    let window = search.best.0;
    if window.is_null() {
        return;
    }
    // SAFETY: `window` is a live top-level window found above.
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        SetForegroundWindow(window);
    }
}

/// The lower-cased full path of process `pid`'s executable.
fn process_executable_path(pid: u32) -> Option<Vec<u16>> {
    // SAFETY: the process handle is only queried and then closed; the buffer outlives the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = vec![0u16; 1024];
        let mut length = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        windows_sys::Win32::Foundation::CloseHandle(process);
        if ok == 0 {
            return None;
        }
        buffer.truncate(length as usize);
        let path = <std::ffi::OsString as std::os::windows::ffi::OsStringExt>::from_wide(&buffer);
        Some(path_key(std::path::Path::new(&path)))
    }
}

/// One spelling per file (long names, one case), so both sides of the comparison agree.
fn path_key(path: &std::path::Path) -> Vec<u16> {
    lowercase_wide(
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .as_os_str(),
    )
}

fn lowercase_wide(name: &std::ffi::OsStr) -> Vec<u16> {
    std::ffi::OsString::from(name.to_string_lossy().to_lowercase())
        .encode_wide()
        .collect()
}
