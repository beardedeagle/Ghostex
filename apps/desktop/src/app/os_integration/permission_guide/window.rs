//! The edge tag's window: after Grant opens a Privacy pane for SpaceO, a Ghostex tag sits on the
//! right edge of System Settings with the `spaceo` file to drag into the list. Dropping it adds
//! SpaceO and switches it on, so the tag waits for that, opens the other permission's pane,
//! restarts SpaceO by itself when SpaceO started before a grant, and tucks away once both work.
//!
//! CDXC:OsIntegration 2026-10-14 DECISION:
//! User picked option A, "Edge tag" (docs/2026-10-14/permission-guide-v3): a tag attached outside System Settings' right edge, its notch pointing into the app list, almost all of it the draggable spaceo tile with orange chevrons toward the list, two progress segments along its top, in Ghostex's own look. User: "dragging it from the floating bar into the app toggles it on", so the flow is built around the drop and has no separate switch step: "Drag SpaceO into the list", Screen Recording first, then Accessibility.
//!
//! SEE-ALSO: apps/desktop/native/macos/GpuiPermissionGuide.m (System Settings' frame and the file
//! drag), helpers/os_cli/tool_permissions.rs (the grant states), ../tool_permissions.rs (Grant),
//! tag.rs (the drawing and each step's words).

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Pixels, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use super::tag::{
    CLOSING, NOTCH_ROOM, NOTCH_Y, TAG_WIDTH, TagActions, TagGrant, TagPermission, TagSide,
    TagStage, WINDOW_WIDTH, render_tag, tag_content, tag_height,
};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The tag's panel starts this far right of System Settings' right edge.
const EDGE_GAP: f32 = 14.0;
/// Where the notch points, below System Settings' top edge: about the end of a Privacy pane's
/// app list. macOS does not say where the list ends without reading its window, which needs
/// Accessibility for Ghostex.
const LIST_END_FROM_TOP: f32 = 236.0;
const TICK: Duration = Duration::from_millis(250);
const PROBE_EVERY: Duration = Duration::from_millis(1500);
/// How long "SpaceO is in and switched on" shows before the next pane opens.
const DROPPED_PAUSE: Duration = Duration::from_millis(1400);
const LIFETIME: Duration = Duration::from_secs(10 * 60);
/// How soon SpaceO's restart is asked for again when the last one ended without picking the
/// grants up (it gave up waiting for a busy agent).
const RESTART_RETRY: Duration = Duration::from_secs(5);

fn permission_from_id(id: &str) -> Option<TagPermission> {
    match id {
        "accessibility" => Some(TagPermission::Accessibility),
        "screenRecording" => Some(TagPermission::ScreenRecording),
        _ => None,
    }
}

fn pane_url(permission: TagPermission) -> &'static str {
    match permission {
        TagPermission::Accessibility => GPUI_MACOS_ACCESSIBILITY_PREFERENCES_URL,
        TagPermission::ScreenRecording => GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL,
    }
}

fn grant(state: GpuiToolPermissionState) -> TagGrant {
    TagGrant {
        has: matches!(
            state,
            GpuiToolPermissionState::Granted | GpuiToolPermissionState::NeedsRestart
        ),
        needs_restart: state == GpuiToolPermissionState::NeedsRestart,
    }
}

pub(crate) struct PermissionGuideView {
    app: WeakEntity<GhostexGpuiApp>,
    native: usize,
    spaceo: PathBuf,
    current: TagPermission,
    accessibility: GpuiToolPermissionState,
    screen_recording: GpuiToolPermissionState,
    stage: TagStage,
    /// When the stage began (Dropped's pause, Ready's countdown).
    stage_since: Instant,
    opened: Instant,
    last_probe: Option<Instant>,
    probing: bool,
    frame: Option<Bounds<Pixels>>,
    side: TagSide,
    visible: bool,
    restart_requested: Option<Instant>,
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
    fn GhostexGpuiPermissionGuideScreenFrame(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        out_x: *mut f64,
        out_y: *mut f64,
        out_width: *mut f64,
        out_height: *mut f64,
    ) -> bool;
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

/// The visible part of the screen System Settings is on.
#[cfg(target_os = "macos")]
fn screen_frame(settings: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
    let [mut x, mut y, mut width, mut height] = [0f64; 4];
    // SAFETY: plain values in, four writable doubles out.
    unsafe {
        GhostexGpuiPermissionGuideScreenFrame(
            f32::from(settings.origin.x) as f64,
            f32::from(settings.origin.y) as f64,
            f32::from(settings.size.width) as f64,
            f32::from(settings.size.height) as f64,
            &mut x,
            &mut y,
            &mut width,
            &mut height,
        )
    }
    .then(|| {
        Bounds::new(
            point(px(x as f32), px(y as f32)),
            size(px(width as f32), px(height as f32)),
        )
    })
}

/// Outside System Settings' right edge with the notch at about the end of its app list, or
/// outside its left edge when the right side would run off the screen.
///
/// CDXC:OsIntegration 2026-10-14 DECISION:
/// User: flip the tag to System Settings' left side when there isn't room on the right, mirroring the notch and chevrons, and re-check as the window moves.
fn tag_frame(
    settings: Bounds<Pixels>,
    screen: Option<Bounds<Pixels>>,
    height: f32,
) -> (Bounds<Pixels>, TagSide) {
    let top = f32::from(settings.origin.y);
    let list_end = top + LIST_END_FROM_TOP.min(f32::from(settings.size.height) - 80.0);
    let right_x = f32::from(settings.origin.x + settings.size.width) + EDGE_GAP - NOTCH_ROOM;
    let fits_right = screen.is_none_or(|screen| {
        right_x + WINDOW_WIDTH <= f32::from(screen.origin.x + screen.size.width)
    });
    let left_x = f32::from(settings.origin.x) - EDGE_GAP - TAG_WIDTH;
    let fits_left = screen.is_some_and(|screen| left_x >= f32::from(screen.origin.x));
    let (x, side) = if fits_right || !fits_left {
        (right_x, TagSide::Right)
    } else {
        (left_x, TagSide::Left)
    };
    (
        Bounds::new(
            point(px(x), px(list_end - NOTCH_Y)),
            size(px(WINDOW_WIDTH), px(height)),
        ),
        side,
    )
}

impl GhostexGpuiApp {
    /// Opens the tag for SpaceO's `permission` (or points the open one at it).
    pub(crate) fn open_gpui_permission_guide(
        &mut self,
        spaceo: PathBuf,
        permission: &str,
        cx: &mut Context<Self>,
    ) {
        if !cfg!(target_os = "macos") {
            return;
        }
        let Some(permission) = permission_from_id(permission) else {
            return;
        };
        let open = GUIDE.with(|guide| *guide.borrow());
        if let Some(handle) = open {
            let pointed = handle.update(cx, |view, _, cx| {
                view.current = permission;
                view.set_stage(TagStage::Waiting);
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
                size(px(WINDOW_WIDTH), px(160.0)),
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
                        stage: TagStage::Waiting,
                        stage_since: Instant::now(),
                        opened: Instant::now(),
                        last_probe: None,
                        probing: false,
                        frame: None,
                        side: TagSide::Right,
                        visible: false,
                        restart_requested: None,
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
                    // SAFETY: the tag's own NSView, alive while its window is.
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
    fn state(&self, permission: TagPermission) -> GpuiToolPermissionState {
        match permission {
            TagPermission::Accessibility => self.accessibility,
            TagPermission::ScreenRecording => self.screen_recording,
        }
    }

    fn set_stage(&mut self, stage: TagStage) {
        self.stage = stage;
        self.stage_since = Instant::now();
    }

    fn content(&self) -> super::tag::TagContent {
        let file_name = self
            .spaceo
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "spaceo".to_string());
        tag_content(
            self.stage,
            self.current,
            grant(self.screen_recording),
            grant(self.accessibility),
            &file_name,
            self.side,
        )
    }

    /// Keeps the tag on System Settings, re-checks the grants, and moves through the steps.
    /// `false` once the tag closed itself.
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.opened.elapsed() >= LIFETIME {
            window.remove_window();
            return false;
        }
        match self.stage {
            TagStage::Dropped if self.stage_since.elapsed() >= DROPPED_PAUSE => {
                let next = self.current.other();
                if grant(self.state(next)).has {
                    self.settle(cx);
                } else {
                    self.current = next;
                    self.set_stage(TagStage::Waiting);
                    let _ = self.app.update(cx, |app, cx| {
                        app.open_gpui_macos_system_settings_url(
                            pane_url(next),
                            "grantToolPermission",
                            cx,
                        );
                    });
                }
                cx.notify();
            }
            TagStage::Ready if self.stage_since.elapsed() >= CLOSING => {
                window.remove_window();
                return false;
            }
            // A restart that ended without picking the grants up (it gave up waiting for a busy
            // agent) is asked for again.
            TagStage::Restarting { .. }
                if !gpui_spaceo_restarting()
                    && [self.accessibility, self.screen_recording]
                        .contains(&GpuiToolPermissionState::NeedsRestart)
                    && self
                        .restart_requested
                        .is_none_or(|asked| asked.elapsed() >= RESTART_RETRY) =>
            {
                self.request_restart(cx);
            }
            _ => {}
        }
        self.follow_system_settings();
        if !self.probing
            && self
                .last_probe
                .is_none_or(|probed| probed.elapsed() >= PROBE_EVERY)
        {
            self.probe(cx);
        }
        true
    }

    /// Both permissions are in: SpaceO restarts by itself when it started before one of them
    /// (once no agent is using it), else done.
    ///
    /// CDXC:OsIntegration 2026-10-14 DECISION:
    /// User: restart SpaceO by itself when no agent is using it instead of asking Restart SpaceO / Later; show "Restarting SpaceO" then "SpaceO is ready", and while an agent is using SpaceO wait for it and say so. Supersedes the earlier ask-first step. The restart is SpaceO's own waiting `daemon restart --operator`; there is no "Restart now", because restarting before the agents finish would end their sessions.
    fn settle(&mut self, cx: &mut Context<Self>) {
        let needs_restart = [self.accessibility, self.screen_recording]
            .contains(&GpuiToolPermissionState::NeedsRestart);
        if needs_restart {
            self.set_stage(TagStage::Restarting { agents: 0 });
            self.request_restart(cx);
        } else {
            self.set_stage(TagStage::Ready);
        }
    }

    fn request_restart(&mut self, cx: &mut Context<Self>) {
        self.restart_requested = Some(Instant::now());
        let _ = self.app.update(cx, |app, cx| app.restart_gpui_spaceo(cx));
    }

    #[cfg(target_os = "macos")]
    fn follow_system_settings(&mut self) {
        let native = self.native as *mut std::ffi::c_void;
        if native.is_null() {
            return;
        }
        let height = tag_height(&self.content());
        let placed = system_settings_frame()
            .map(|settings| tag_frame(settings, screen_frame(settings), height));
        match placed {
            Some((frame, side)) => {
                self.side = side;
                if self.frame != Some(frame) {
                    self.frame = Some(frame);
                    // SAFETY: the tag's own NSView, alive while its window is.
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
            // System Settings closed, minimized or on another desktop: the tag waits out of sight.
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

    /// The same fresh-process check Settings uses, so a drop shows the moment macOS has it.
    fn probe(&mut self, cx: &mut Context<Self>) {
        self.probing = true;
        self.last_probe = Some(Instant::now());
        let spaceo = self.spaceo.clone();
        let restarting = matches!(self.stage, TagStage::Restarting { .. });
        let report = cx.background_executor().spawn(async move {
            let sessions = restarting
                .then(|| gpui_spaceo_live_sessions(&spaceo))
                .flatten();
            (gpui_spaceo_permission_report(&spaceo), sessions)
        });
        cx.spawn(async move |view, cx| {
            let (report, sessions) = report.await;
            let _ = view.update(cx, |view, cx| {
                view.probing = false;
                let changed = view.accessibility != report.accessibility
                    || view.screen_recording != report.screen_recording;
                view.accessibility = report.accessibility;
                view.screen_recording = report.screen_recording;
                match view.stage {
                    TagStage::Waiting if grant(view.state(view.current)).has => {
                        view.set_stage(TagStage::Dropped);
                    }
                    TagStage::Restarting { .. }
                        if [view.accessibility, view.screen_recording]
                            .iter()
                            .all(|state| *state == GpuiToolPermissionState::Granted) =>
                    {
                        view.set_stage(TagStage::Ready);
                    }
                    TagStage::Restarting { agents } => {
                        let now = sessions.unwrap_or(agents);
                        if now != agents {
                            view.stage = TagStage::Restarting { agents: now };
                        }
                    }
                    _ => {}
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

    fn begin_drag(&self) {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::ffi::OsStrExt;
            let Ok(path) = std::ffi::CString::new(self.spaceo.as_os_str().as_bytes()) else {
                return;
            };
            // SAFETY: the tag's own NSView and a live C string; called inside its mouse-down.
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
        render_tag(
            self.content(),
            TagActions {
                drag: Box::new(|view: &mut Self, _, _| view.begin_drag()),
                close: Box::new(|_, window, _| window.remove_window()),
            },
            cx,
        )
    }
}
