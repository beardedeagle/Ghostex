use std::{
    env, fs, io,
    io::Write as _,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};
use tao::window::Window;
use tempfile::NamedTempFile;

pub(crate) fn write_draft_atomically(path: &Path, draft: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(directory)?;
    temp.write_all(draft.as_bytes())?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map(|_| ()).map_err(|error| error.error)
}

#[derive(Clone, Copy)]
pub(crate) struct WindowFrame {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

fn window_frame_store_path() -> Option<PathBuf> {
    Some(resolved_state_directory().join("editor-window-frame.json"))
}

pub(crate) fn absolute_environment_path(variable: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn resolved_state_directory() -> PathBuf {
    if let Some(ghostex_home) = absolute_environment_path("GHOSTEX_HOME") {
        return ghostex_home.join("state");
    }
    let home = absolute_environment_path("HOME").unwrap_or_else(|| PathBuf::from("."));
    absolute_environment_path("XDG_STATE_HOME")
        .unwrap_or_else(|| home.join(".local/state"))
        .join("ghostex")
}

#[cfg(target_os = "windows")]
pub(crate) fn resolved_state_directory() -> PathBuf {
    if let Some(ghostex_home) = absolute_environment_path("GHOSTEX_HOME") {
        return ghostex_home.join("state");
    }
    if let Some(copy) = ghostex_paths::portable::current() {
        return copy.data_dir.join("state");
    }
    let user_home = absolute_environment_path("USERPROFILE").unwrap_or_else(|| PathBuf::from("."));
    absolute_environment_path("LOCALAPPDATA")
        .unwrap_or_else(|| user_home.join("AppData/Local"))
        .join("Ghostex/State")
}

/// CDXC:PromptEditor 2026-09-22 WHY:
/// Without an explicit user data folder WebView2 writes its profile next to the executable
/// (`GhostexEditor.exe.WebView2`), which fails under `C:\Program Files` with the Edge dialog
/// "We couldn't create the data directory". The profile is a cache, so it lives under the
/// per-user `GhostexData` root that the installer never replaces, beside the component store,
/// rather than under `%LOCALAPPDATA%\Ghostex` which Velopack swaps out while the daemon may
/// still hold WebView2 file locks.
#[cfg(target_os = "windows")]
pub(crate) fn windows_webview_data_directory() -> PathBuf {
    if let Some(ghostex_home) = absolute_environment_path("GHOSTEX_HOME") {
        return ghostex_home.join("state/editor-webview2");
    }
    if let Some(copy) = ghostex_paths::portable::current() {
        return copy.data_dir.join("state/editor-webview2");
    }
    let user_home = absolute_environment_path("USERPROFILE").unwrap_or_else(|| PathBuf::from("."));
    absolute_environment_path("LOCALAPPDATA")
        .unwrap_or_else(|| user_home.join("AppData/Local"))
        .join("GhostexData/editor-webview2")
}

pub(crate) fn load_saved_window_frame() -> Option<WindowFrame> {
    let path = window_frame_store_path()?;
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let field = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|number| number.is_finite())
    };
    let frame = WindowFrame {
        x: field("x")?,
        y: field("y")?,
        width: field("width")?,
        height: field("height")?,
    };
    (frame.width > 0.0 && frame.height > 0.0).then_some(frame)
}

pub(crate) fn save_window_frame(window: &Window) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Some(path) = window_frame_store_path() else {
        return;
    };
    let scale = window.scale_factor();
    let logical_position = position.to_logical::<f64>(scale);
    let logical_size = window.inner_size().to_logical::<f64>(scale);
    let value = json!({
        "x": logical_position.x,
        "y": logical_position.y,
        "width": logical_size.width,
        "height": logical_size.height,
    });
    if let Err(error) = write_draft_atomically(&path, &value.to_string()) {
        eprintln!("ghostex-editor: window frame save failed: {error}");
    }
}

pub(crate) fn write_status(path: &Path, status: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = write_draft_atomically(path, status) {
        eprintln!(
            "ghostex-editor: status write failed for {}: {error}",
            path.display()
        );
    }
}
