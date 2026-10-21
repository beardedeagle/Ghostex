//! Where the button sits on each screen, whether it is hidden, and where the last prompt went,
//! kept in `ghostex-capture-state.json` in the state folder like the main window's frame.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::model::DockEdge;
use crate::app::helpers::ghostex_state_root;

/// The icon's place on one screen, relative to that screen's top-left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedPlacement {
    pub(crate) x: f32,
    pub(crate) y: f32,
    #[serde(default)]
    pub(crate) dock: Option<DockEdge>,
}

/// The last area captured: the screen it was on and the box in points from that screen's corner.
///
/// CDXC:GhostexCapture 2026-09-30 DECISION:
/// User: an area screenshot starts with a resizable box where the last capture was; dragging from
/// outside it selects another area, and Enter or A captures the box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedArea {
    pub(crate) display: String,
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
}

/// Where one of Ghostex Capture's windows was last left: its screen, its top-left corner from
/// that screen's corner, and its size for a window the user can resize.
///
/// CDXC:GhostexCapture 2026-09-30 DECISION:
/// User: "we keep shifting windows around PLEASE make the sessions list window and the screenshot
/// window and the floating prompt editor window all stay same spot", "remember last position for
/// the prompt window and when there's no last position put it center of the current screen", and
/// "make the screenshot editing thingy resizable and remember its size". Each window opens where it
/// was last left and does not move by itself afterwards.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedWindowFrame {
    pub(crate) display: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    #[serde(default)]
    pub(crate) width: Option<f32>,
    #[serde(default)]
    pub(crate) height: Option<f32>,
}

/// CDXC:GhostexCapture 2026-09-30 DECISION:
/// User: the button's position is remembered per screen, and "Hide button" keeps it hidden until
/// it is turned back on in Settings or with the Cmd/Alt+Ctrl+Shift+S hotkey.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedCaptureState {
    #[serde(default)]
    pub(crate) hidden: bool,
    /// The screen the button was last on.
    #[serde(default)]
    pub(crate) display: Option<String>,
    #[serde(default)]
    pub(crate) placements: BTreeMap<String, SavedPlacement>,
    /// The box of the last area capture, shown again on the next one.
    #[serde(default)]
    pub(crate) last_area: Option<SavedArea>,
    #[serde(default)]
    pub(crate) prompt_position: Option<SavedWindowFrame>,
    /// The Running Agents panel, once the user moved it away from the button.
    #[serde(default)]
    pub(crate) panel_position: Option<SavedWindowFrame>,
    #[serde(default)]
    pub(crate) editor_frame: Option<SavedWindowFrame>,
}

fn state_path() -> PathBuf {
    ghostex_state_root().join("ghostex-capture-state.json")
}

pub(crate) fn load() -> SavedCaptureState {
    std::fs::read(state_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub(crate) fn save(state: &SavedCaptureState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(state) {
        let temporary = path.with_extension("json.tmp");
        if std::fs::write(&temporary, bytes).is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
    }
}
