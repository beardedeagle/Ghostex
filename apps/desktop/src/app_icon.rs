use std::{
    cmp::Ordering,
    ffi::{CStr, CString},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::{Map, Value};

const APP_ICON_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const APP_ICON_MAX_SOURCE_DIMENSION: u32 = 2048;
const APP_ICON_MAX_SOURCE_ID_CHARS: usize = 255;
const APP_ICON_LIST_LIMIT: usize = 10;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/*
CDXC:Icons 2026-07-12:
The GPUI app icon backend shares the app's settings key and resolved Ghostex
icons directory. Rust keeps all persisted/renderer identifiers as
validated filenames, scans only bounded valid PNGs, and copies picker results
under unique sanitized names. AppKit receives absolute paths only inside the
native process for image decoding/masking; paths and image bytes never enter
settings, logs, or the modal contract.
*/

pub(crate) fn source_id_from_settings(settings: &Map<String, Value>) -> String {
    settings
        .get("appIconSourceId")
        .and_then(Value::as_str)
        .and_then(normalized_source_id)
        .unwrap_or_default()
}

/// CDXC:Theming 2026-10-21 DECISION:
/// User: "fully hide the custom app icon feature" (2026-09-23); on 2026-10-21 the user removed the hidden App Icon picker and its catalog section outright. `appIconSourceId` stays, so an icon that was already chosen keeps working, and Reset all settings still restores the bundled icon through `setAppIcon`.
pub(crate) fn apply_persisted_source_id(source_id: &str) -> bool {
    let normalized = normalized_source_id(source_id).unwrap_or_default();
    if normalized.is_empty() {
        return apply_native_icon(None);
    }
    let path = icons_directory().join(&normalized);
    if !is_valid_source_png(&path) {
        return apply_native_icon(None);
    }
    apply_native_icon(Some(&path))
}

pub(crate) fn select_state(source_id: &str, current_source_id: &str) -> Value {
    let trimmed = source_id.trim();
    if trimmed.is_empty() {
        return if apply_native_icon(None) {
            app_icon_state("", true, None)
        } else {
            app_icon_state(current_source_id, false, Some("iconUnavailable"))
        };
    }
    let Some(normalized) = normalized_source_id(trimmed) else {
        return app_icon_state(current_source_id, false, Some("iconUnavailable"));
    };
    let path = icons_directory().join(&normalized);
    if !is_valid_source_png(&path) || !apply_native_icon(Some(&path)) {
        return app_icon_state(current_source_id, false, Some("iconUnavailable"));
    }
    app_icon_state(&normalized, true, None)
}

fn app_icon_state(requested_source_id: &str, ok: bool, error: Option<&str>) -> Value {
    let directory = icons_directory();
    let _ = fs::create_dir_all(&directory);
    let selected_id = available_source_id(requested_source_id).unwrap_or_default();
    let mut icons = vec![serde_json::json!({
        "id": "",
        "name": "Default",
        "selected": selected_id.is_empty(),
        "thumbnailDataUrl": native_thumbnail_data_url(None),
    })];
    for source_id in scanned_source_ids(&directory, &selected_id) {
        let path = directory.join(&source_id);
        let thumbnail = native_thumbnail_data_url(Some(&path));
        if thumbnail.is_empty() {
            continue;
        }
        icons.push(serde_json::json!({
            "id": source_id,
            "name": display_name(&source_id),
            "selected": source_id == selected_id,
            "thumbnailDataUrl": thumbnail,
        }));
    }
    serde_json::json!({
        "error": error,
        "icons": icons,
        "ok": ok,
        "selectedId": selected_id,
        "type": "appIconState",
    })
}

fn available_source_id(source_id: &str) -> Option<String> {
    let normalized = normalized_source_id(source_id)?;
    is_valid_source_png(&icons_directory().join(&normalized)).then_some(normalized)
}

fn normalized_source_id(source_id: &str) -> Option<String> {
    let trimmed = source_id.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }
    if trimmed.chars().count() > APP_ICON_MAX_SOURCE_ID_CHARS
        || matches!(trimmed, "." | "..")
        || trimmed.contains(['/', '\\', '\0'])
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn icons_directory() -> PathBuf {
    crate::shared_settings::ghostex_storage_paths().icons_dir()
}

fn scanned_source_ids(directory: &Path, selected_id: &str) -> Vec<String> {
    let mut entries = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let source_id = path.file_name()?.to_str()?.to_string();
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
                || normalized_source_id(&source_id).as_deref() != Some(source_id.as_str())
                || !is_valid_source_png(&path)
            {
                return None;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((source_id, modified))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| match right.1.cmp(&left.1) {
        Ordering::Equal => left.0.cmp(&right.0),
        ordering => ordering,
    });
    let mut source_ids = entries
        .into_iter()
        .take(APP_ICON_LIST_LIMIT)
        .map(|entry| entry.0)
        .collect::<Vec<_>>();
    if !selected_id.is_empty()
        && !source_ids.iter().any(|source_id| source_id == selected_id)
        && is_valid_source_png(&directory.join(selected_id))
    {
        source_ids.push(selected_id.to_string());
    }
    source_ids
}

fn is_valid_source_png(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > APP_ICON_MAX_FILE_BYTES {
        return false;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 24];
    if file.read_exact(&mut header).is_err()
        || header[..8] != PNG_SIGNATURE
        || &header[12..16] != b"IHDR"
    {
        return false;
    }
    let width = u32::from_be_bytes(header[16..20].try_into().unwrap_or_default());
    let height = u32::from_be_bytes(header[20..24].try_into().unwrap_or_default());
    width > 0
        && height > 0
        && width <= APP_ICON_MAX_SOURCE_DIMENSION
        && height <= APP_ICON_MAX_SOURCE_DIMENSION
}

fn display_name(source_id: &str) -> String {
    let base = source_id
        .strip_suffix(".png")
        .or_else(|| source_id.strip_suffix(".PNG"))
        .unwrap_or(source_id);
    let display = base.replace(['-', '_'], " ").trim().to_string();
    if display.is_empty() {
        source_id.to_string()
    } else {
        display
    }
}

#[cfg(target_os = "macos")]
fn apply_native_icon(path: Option<&Path>) -> bool {
    unsafe extern "C" {
        fn GhostexGpuiAppIconApplyFile(path: *const std::ffi::c_char) -> std::ffi::c_int;
    }
    let path = path.and_then(path_c_string);
    unsafe {
        GhostexGpuiAppIconApplyFile(path.as_ref().map_or(std::ptr::null(), |path| path.as_ptr()))
            == 1
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_native_icon(_path: Option<&Path>) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn native_thumbnail_data_url(path: Option<&Path>) -> String {
    unsafe extern "C" {
        fn GhostexGpuiAppIconThumbnailDataUrl(
            path: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char;
        fn GhostexGpuiAppIconFreeCString(value: *mut std::ffi::c_char);
    }
    let path = path.and_then(path_c_string);
    let value = unsafe {
        GhostexGpuiAppIconThumbnailDataUrl(
            path.as_ref().map_or(std::ptr::null(), |path| path.as_ptr()),
        )
    };
    if value.is_null() {
        return String::new();
    }
    let result = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .to_string();
    unsafe { GhostexGpuiAppIconFreeCString(value) };
    result
}

#[cfg(not(target_os = "macos"))]
fn native_thumbnail_data_url(_path: Option<&Path>) -> String {
    String::new()
}

fn path_c_string(path: &Path) -> Option<CString> {
    CString::new(path.to_string_lossy().as_bytes()).ok()
}
