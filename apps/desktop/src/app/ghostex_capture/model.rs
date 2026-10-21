//! Ghostex Capture's state and its fixed sizes.

use std::time::Instant;

use gpui::{Bounds, Pixels, Point, WindowHandle};
use serde::{Deserialize, Serialize};

use super::editor::EditorWindow;
use super::icon_window::CaptureIconView;
use super::overlay::AreaSession;
use super::persistence::SavedCaptureState;
use super::platform::{FrontmostApp, NativeWindow};
use super::prompt::PromptState;

/// The app icon's drawn size.
pub(crate) const ICON_SIZE: f32 = 44.0;
/// Room around the icon for its shadow.
pub(crate) const ICON_PAD: f32 = 7.0;
/// How far the counts tray tucks under the icon's edge.
pub(crate) const TRAY_TUCK: f32 = 10.0;
/// The docked tab's width: about a fifth of the button.
pub(crate) const TAB_WIDTH: f32 = 24.0;
/// How close to a screen edge a drop docks the button.
pub(crate) const DOCK_SNAP: f32 = 28.0;
/// A press that moves less than this is a click.
pub(crate) const DRAG_THRESHOLD: f32 = 4.0;
pub(crate) const PANEL_WIDTH: f32 = 440.0;
pub(crate) const PANEL_GAP: f32 = 6.0;

/// What a click, a panel button, or a hotkey asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureAction {
    TogglePanel,
    Area,
    CurrentApp,
    FullScreen,
    /// A fresh prompt.
    Prompt,
    /// Reopens the prompt box on the unsent draft.
    ContinuePrompt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DockEdge {
    Left,
    Right,
}

/// The three numbers the button shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CaptureCounts {
    pub(crate) working: u64,
    pub(crate) attention: u64,
    pub(crate) question: u64,
}

/// How the button is laid out in its window right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum IconLayout {
    /// Free on the screen; the tray sits on the left when there is no room on the right.
    Floating { tray_left: bool },
    /// Tucked against a screen edge as a thin tab.
    Docked(DockEdge),
    /// Docked, but slid out while the pointer is over it.
    DockedOpen(DockEdge),
}

pub(crate) struct IconWindow {
    pub(crate) handle: WindowHandle<CaptureIconView>,
    pub(crate) native: NativeWindow,
    pub(crate) scale: f32,
    pub(crate) frame: Bounds<Pixels>,
    pub(crate) layout: IconLayout,
}

pub(crate) struct PanelWindow {
    pub(crate) handle: WindowHandle<gpui_component::Root>,
    pub(crate) native: Option<NativeWindow>,
    /// Where it opened, to tell whether the user moved it.
    pub(crate) opened_at: Bounds<Pixels>,
}

/// A press on the button, until it is released.
pub(crate) struct IconPress {
    pub(crate) pointer: Point<Pixels>,
    /// Where the icon itself (not the window) was when the press started.
    pub(crate) icon_origin: Point<Pixels>,
    pub(crate) dragging: bool,
}

#[derive(Default)]
pub(crate) struct GhostexCaptureState {
    pub(crate) enabled: bool,
    pub(crate) saved: SavedCaptureState,
    pub(crate) icon: Option<IconWindow>,
    /// An open is deferred to outside the current update; set while it is on its way.
    pub(crate) icon_opening: bool,
    pub(crate) panel: Option<PanelWindow>,
    pub(crate) panel_opening: bool,
    pub(crate) press: Option<IconPress>,
    pub(crate) hovered: bool,
    pub(crate) hover_left_at: Option<Instant>,
    pub(crate) hotkeys_registered: bool,
    /// The app the user was in when Ghostex Capture was last called up.
    pub(crate) frontmost: FrontmostApp,
    /// A capture is under way: Ghostex Capture's own windows are moved off screen or hidden until it ends.
    pub(crate) capturing: bool,
    /// macOS was asked for Screen Recording once this run; it shows its prompt only once.
    pub(crate) screen_access_asked: bool,
    pub(crate) area: Option<AreaSession>,
    pub(crate) editor: Option<EditorWindow>,
    pub(crate) prompt: PromptState,
}
