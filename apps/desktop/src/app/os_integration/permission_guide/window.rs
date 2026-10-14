//! The floating permission bar: after Grant opens a Privacy pane for SpaceO, a Ghostex bar sits on
//! the edge of the System Settings window with a tile of the `spaceo` file to drag into the list,
//! the switch to turn on, and the grant's live status. It moves on to the other permission once
//! this one lands, restarts SpaceO if it needs to pick a grant up, and closes when both are done.
//!
//! CDXC:OsIntegration 2026-10-14 DECISION:
//! User: "it has to be floating bar i can drag the application from to the settings window please! ... but the look needs to be different." The bar docks above System Settings (below it when there is no room), uses Ghostex's own dark panel, and its tile drags the real file macOS lists.
//!
//! SEE-ALSO: apps/desktop/native/macos/GpuiPermissionGuide.m (System Settings' frame and the file
//! drag), helpers/os_cli/tool_permissions.rs (the grant states), ../tool_permissions.rs (Grant),
//! bar.rs (the drawing).

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Pixels, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use super::bar::{BAR_HEIGHT, BAR_WIDTH, BarContent, BarStep, render_bar};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The gap between the bar and the System Settings window.
const BAR_GAP: f32 = 10.0;
/// Room the menu bar takes at the top of the screen; the bar goes below the window above it.
const MENU_BAR_ROOM: f32 = 34.0;
const TICK: Duration = Duration::from_millis(250);
const PROBE_EVERY: Duration = Duration::from_millis(1500);
const GRANTED_PAUSE: Duration = Duration::from_millis(1400);
const FINISHED_PAUSE: Duration = Duration::from_millis(2500);
const LIFETIME: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GuidePermission {
    Accessibility,
    ScreenRecording,
}

impl GuidePermission {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "accessibility" => Some(Self::Accessibility),
            "screenRecording" => Some(Self::ScreenRecording),
            _ => None,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Accessibility => Self::ScreenRecording,
            Self::ScreenRecording => Self::Accessibility,
        }
    }

    fn pane(self) -> &'static str {
        match self {
            Self::Accessibility => "Accessibility",
            Self::ScreenRecording => "Screen Recording",
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::Accessibility => GPUI_MACOS_ACCESSIBILITY_PREFERENCES_URL,
            Self::ScreenRecording => GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Waiting,
    /// The current permission just landed; the bar says so before moving on.
    Granted(Instant),
    /// Both permissions are in; the bar closes shortly.
    Finished(Instant),
}

/// macOS has the grant, whether or not SpaceO's daemon picked it up yet.
fn has(state: GpuiToolPermissionState) -> bool {
    matches!(
        state,
        GpuiToolPermissionState::Granted | GpuiToolPermissionState::NeedsRestart
    )
}

pub(crate) struct PermissionGuideView {
    app: WeakEntity<GhostexGpuiApp>,
    native: usize,
    spaceo: PathBuf,
    current: GuidePermission,
    accessibility: GpuiToolPermissionState,
    screen_recording: GpuiToolPermissionState,
    phase: Phase,
    opened: Instant,
    last_probe: Option<Instant>,
    probing: bool,
    frame: Option<Bounds<Pixels>>,
    visible: bool,
    restart_requested: bool,
}

thread_local! {
    static GUIDE: RefCell<Option<WindowHandle<PermissionGuideView>>> = const { RefCell::new(None) };
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn GhostexGpuiSystemSettingsWindowFrame(
        x: *mut f64,
        y: *mut f64,
        width: *mut f64,
        height: *mut f64,
    ) -> bool;
    fn GhostexGpuiPermissionGuideSetVisible(native_view: *mut std::ffi::c_void, visible: bool);
    fn GhostexGpuiPermissionGuideBeginFileDrag(
        native_view: *mut std::ffi::c_void,
        path: *const std::ffi::c_char,
    ) -> bool;
    // Ghostex Capture's floating-window calls (GpuiGhostexCapture.m).
    fn GhostexGpuiCapturePrepareFloatingWindow(native_view: *mut std::ffi::c_void, keyable: bool);
    fn GhostexGpuiCaptureSetWindowFrame(
        native_view: *mut std::ffi::c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    );
}

#[cfg(target_os = "macos")]
fn system_settings_frame() -> Option<Bounds<Pixels>> {
    let [mut x, mut y, mut width, mut height] = [0f64; 4];
    // SAFETY: four writable doubles.
    unsafe { GhostexGpuiSystemSettingsWindowFrame(&mut x, &mut y, &mut width, &mut height) }.then(
        || {
            Bounds::new(
                point(px(x as f32), px(y as f32)),
                size(px(width as f32), px(height as f32)),
            )
        },
    )
}

/// Above the window's right edge, where the Privacy list's switches are; below it when the window
/// reaches the menu bar.
fn bar_frame(settings: Bounds<Pixels>) -> Bounds<Pixels> {
    let right = f32::from(settings.origin.x + settings.size.width);
    let x = (right - BAR_WIDTH)
        .max(f32::from(settings.origin.x))
        .max(8.0);
    let above = f32::from(settings.origin.y) - BAR_GAP - BAR_HEIGHT;
    let y = if above >= MENU_BAR_ROOM {
        above
    } else {
        f32::from(settings.origin.y + settings.size.height) + BAR_GAP
    };
    Bounds::new(point(px(x), px(y)), size(px(BAR_WIDTH), px(BAR_HEIGHT)))
}

impl GhostexGpuiApp {
    /// Opens the bar for SpaceO's `permission` (or points the open one at it).
    pub(crate) fn open_gpui_permission_guide(
        &mut self,
        spaceo: PathBuf,
        permission: &str,
        cx: &mut Context<Self>,
    ) {
        if !cfg!(target_os = "macos") {
            return;
        }
        let Some(permission) = GuidePermission::from_id(permission) else {
            return;
        };
        let open = GUIDE.with(|guide| *guide.borrow());
        if let Some(handle) = open {
            let pointed = handle.update(cx, |view, _, cx| {
                view.current = permission;
                view.phase = Phase::Waiting;
                view.opened = Instant::now();
                view.last_probe = None;
                cx.notify();
            });
            if pointed.is_ok() {
                return;
            }
            GUIDE.with(|guide| guide.borrow_mut().take());
        }
        let app = cx.weak_entity();
        // GPUI draws a new window's root synchronously; opening it inside this entity's update
        // would lease the app twice.
        App::defer(cx, move |cx| {
            let start = Bounds::new(
                point(px(-10_000.0), px(-10_000.0)),
                size(px(BAR_WIDTH), px(BAR_HEIGHT)),
            );
            let (bounds, display_id) = crate::app::window::popup_frame::place_global(start, cx);
            let view_app = app.clone();
            let spaceo_for_view = spaceo.clone();
            let Ok(handle) = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: false,
                    show: false,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |window, cx| {
                    crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                    let native = crate::app::helpers::cef_parent_native_view(window)
                        .map(|view| view as usize)
                        .unwrap_or_default();
                    cx.new(|_| PermissionGuideView {
                        app: view_app,
                        native,
                        spaceo: spaceo_for_view,
                        current: permission,
                        accessibility: GpuiToolPermissionState::Unknown,
                        screen_recording: GpuiToolPermissionState::Unknown,
                        phase: Phase::Waiting,
                        opened: Instant::now(),
                        last_probe: None,
                        probing: false,
                        frame: None,
                        visible: false,
                        restart_requested: false,
                    })
                },
            ) else {
                return;
            };
            #[cfg(target_os = "macos")]
            {
                let native = handle
                    .update(cx, |view, _, _| view.native)
                    .unwrap_or_default();
                if native != 0 {
                    // SAFETY: the bar's own NSView, alive while its window is.
                    unsafe {
                        GhostexGpuiCapturePrepareFloatingWindow(
                            native as *mut std::ffi::c_void,
                            false,
                        );
                        GhostexGpuiPermissionGuideSetVisible(
                            native as *mut std::ffi::c_void,
                            false,
                        );
                    }
                }
            }
            GUIDE.with(|guide| *guide.borrow_mut() = Some(handle));
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(TICK).await;
                    let alive = handle.update(cx, |view, window, cx| view.tick(window, cx));
                    if !matches!(alive, Ok(true)) {
                        break;
                    }
                }
                GUIDE.with(|guide| {
                    if *guide.borrow() == Some(handle) {
                        guide.borrow_mut().take();
                    }
                });
            })
            .detach();
        });
    }
}

impl PermissionGuideView {
    fn state(&self, permission: GuidePermission) -> GpuiToolPermissionState {
        match permission {
            GuidePermission::Accessibility => self.accessibility,
            GuidePermission::ScreenRecording => self.screen_recording,
        }
    }

    /// Keeps the bar on System Settings, re-checks the grants, and moves through the steps.
    /// `false` once the bar closed itself.
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.opened.elapsed() >= LIFETIME {
            window.remove_window();
            return false;
        }
        self.follow_system_settings();
        match self.phase {
            Phase::Granted(at) if at.elapsed() >= GRANTED_PAUSE => {
                let next = self.current.other();
                if has(self.state(next)) {
                    self.finish(cx);
                } else {
                    self.current = next;
                    self.phase = Phase::Waiting;
                    let _ = self.app.update(cx, |app, cx| {
                        app.open_gpui_macos_system_settings_url(
                            next.url(),
                            "grantToolPermission",
                            cx,
                        );
                    });
                }
                cx.notify();
            }
            Phase::Finished(at) if at.elapsed() >= FINISHED_PAUSE => {
                window.remove_window();
                return false;
            }
            _ => {}
        }
        if !self.probing
            && self
                .last_probe
                .is_none_or(|probed| probed.elapsed() >= PROBE_EVERY)
        {
            self.probe(cx);
        }
        true
    }

    #[cfg(target_os = "macos")]
    fn follow_system_settings(&mut self) {
        let native = self.native as *mut std::ffi::c_void;
        if native.is_null() {
            return;
        }
        match system_settings_frame().map(bar_frame) {
            Some(frame) => {
                if self.frame != Some(frame) {
                    self.frame = Some(frame);
                    // SAFETY: the bar's own NSView, alive while its window is.
                    unsafe {
                        GhostexGpuiCaptureSetWindowFrame(
                            native,
                            f32::from(frame.origin.x) as f64,
                            f32::from(frame.origin.y) as f64,
                            f32::from(frame.size.width) as f64,
                            f32::from(frame.size.height) as f64,
                        )
                    };
                }
                if !self.visible {
                    self.visible = true;
                    // SAFETY: as above.
                    unsafe { GhostexGpuiPermissionGuideSetVisible(native, true) };
                }
            }
            // System Settings closed, minimized or on another Space: the bar waits out of sight.
            None if self.visible => {
                self.visible = false;
                // SAFETY: as above.
                unsafe { GhostexGpuiPermissionGuideSetVisible(native, false) };
            }
            None => {}
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn follow_system_settings(&mut self) {}

    /// The same fresh-process check Settings uses, so a grant shows the moment macOS has it.
    fn probe(&mut self, cx: &mut Context<Self>) {
        self.probing = true;
        self.last_probe = Some(Instant::now());
        let spaceo = self.spaceo.clone();
        let report = cx
            .background_executor()
            .spawn(async move { gpui_spaceo_permission_report(&spaceo) });
        cx.spawn(async move |view, cx| {
            let report = report.await;
            let _ = view.update(cx, |view, cx| {
                view.probing = false;
                let changed = view.accessibility != report.accessibility
                    || view.screen_recording != report.screen_recording;
                view.accessibility = report.accessibility;
                view.screen_recording = report.screen_recording;
                if view.phase == Phase::Waiting && has(view.state(view.current)) {
                    view.phase = Phase::Granted(Instant::now());
                }
                if changed {
                    // Settings' rows follow without waiting for their own re-check.
                    let _ = view
                        .app
                        .update(cx, |app, cx| app.refresh_gpui_tool_permission_status(cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Both grants are in: restart SpaceO if its daemon started without one, then close.
    fn finish(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Finished(Instant::now());
        let needs_restart = [self.accessibility, self.screen_recording]
            .contains(&GpuiToolPermissionState::NeedsRestart);
        if needs_restart && !self.restart_requested {
            self.restart_requested = true;
            let _ = self.app.update(cx, |app, cx| app.restart_gpui_spaceo(cx));
        }
    }

    fn begin_drag(&self) {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::ffi::OsStrExt;
            let Ok(path) = std::ffi::CString::new(self.spaceo.as_os_str().as_bytes()) else {
                return;
            };
            // SAFETY: the bar's own NSView and a live C string; called inside its mouse-down.
            unsafe {
                GhostexGpuiPermissionGuideBeginFileDrag(
                    self.native as *mut std::ffi::c_void,
                    path.as_ptr(),
                )
            };
        }
    }
}

impl Render for PermissionGuideView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = self.current.pane();
        let landed = !matches!(self.phase, Phase::Waiting);
        let finished = matches!(self.phase, Phase::Finished(_));
        let (title, detail) = match self.phase {
            Phase::Waiting => (
                format!("{pane} for SpaceO"),
                "Switch on spaceo. Not listed? Drag the tile in.".to_string(),
            ),
            Phase::Granted(_) => (
                format!("{pane} is on"),
                format!("Next: {}.", self.current.other().pane()),
            ),
            Phase::Finished(_) => (
                "SpaceO is all set".to_string(),
                if self.restart_requested {
                    "SpaceO restarts once the agents using it finish.".to_string()
                } else {
                    "Agents can use SpaceO now.".to_string()
                },
            ),
        };
        let steps = [
            GuidePermission::Accessibility,
            GuidePermission::ScreenRecording,
        ]
        .map(|permission| {
            if has(self.state(permission)) || (permission == self.current && landed) {
                BarStep::Done
            } else if permission == self.current {
                BarStep::Current
            } else {
                BarStep::Todo
            }
        });
        let file_name = self
            .spaceo
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "spaceo".to_string());
        render_bar(
            BarContent {
                title,
                detail,
                file_name,
                landed,
                step: (!finished).then_some(match self.current {
                    GuidePermission::Accessibility => 1,
                    GuidePermission::ScreenRecording => 2,
                }),
                steps,
            },
            |view: &mut Self, _, _| view.begin_drag(),
            |_, window, _| window.remove_window(),
            cx,
        )
    }
}
