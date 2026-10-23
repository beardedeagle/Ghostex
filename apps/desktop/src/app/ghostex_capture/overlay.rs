//! "Screenshot an area": every screen is frozen as it was, dimmed, and the user drags a box on any
//! of them.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: with more than one screen, dim every screen and let the drag happen on any of them.
//!
//! The screens are captured first and the overlay draws those pictures, so the selection shows
//! exactly the pixels that will be cropped and nothing moving underneath can change them.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, CursorStyle, FocusHandle, ImageSource, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderImage, Subscription,
    WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, div, img, point, px, rgb, rgba, size,
};
use image::RgbaImage;

use super::editor::model::{Area, BOX_HANDLES, Handle, P, drag_handle, handle_point};
use super::model::CaptureAction;
use super::placement::Screen;
use super::platform;
use crate::GhostexGpuiApp;

/// How near (points) a press must be to a handle to grab it.
const HANDLE_SLOP: f32 = 8.0;

pub(crate) struct AreaScreen {
    pub(crate) screen: Screen,
    pub(crate) image: Arc<RgbaImage>,
    pub(crate) render: Arc<RenderImage>,
    pub(crate) window: Option<WindowHandle<AreaOverlayView>>,
}

#[derive(Clone, Copy)]
enum AreaDrag {
    New { from: P },
    Handle { handle: Handle, start: Area },
    Move { from: P, start: Area },
}

pub(crate) struct AreaSession {
    pub(crate) screens: Vec<AreaScreen>,
    /// The box, in points from the top-left corner of the screen it is on.
    selection: Option<(usize, Area)>,
    drag: Option<AreaDrag>,
}

fn to_render_image(image: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = image.clone();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(bgra)]))
}

impl GhostexGpuiApp {
    pub(super) fn open_ghostex_capture_area(
        &mut self,
        grabs: Vec<(Screen, RgbaImage)>,
        cx: &mut Context<Self>,
    ) {
        let screens: Vec<AreaScreen> = grabs
            .into_iter()
            .map(|(screen, image)| AreaScreen {
                render: to_render_image(&image),
                image: Arc::new(image),
                screen,
                window: None,
            })
            .collect();
        if screens.is_empty() {
            self.finish_ghostex_capture(cx);
            return;
        }
        let pointer = self
            .ghostex_capture
            .icon
            .as_ref()
            .and_then(|icon| platform::pointer(icon.scale));
        let frames: Vec<(usize, Screen)> = screens
            .iter()
            .enumerate()
            .map(|(index, entry)| (index, entry.screen.clone()))
            .collect();
        // The last capture's box comes back on its screen, still resizable.
        let selection = self
            .ghostex_capture
            .saved
            .last_area
            .as_ref()
            .and_then(|last| {
                let index = screens
                    .iter()
                    .position(|entry| entry.screen.key == last.display)?;
                let bounds = screens[index].screen.bounds;
                let area = Area {
                    left: last.left,
                    top: last.top,
                    right: last.right,
                    bottom: last.bottom,
                }
                .clamp_to(f32::from(bounds.size.width), f32::from(bounds.size.height));
                (area.width() >= 3.0 && area.height() >= 3.0).then_some((index, area))
            });
        self.ghostex_capture.area = Some(AreaSession {
            screens,
            selection,
            drag: None,
        });
        let app = cx.weak_entity();
        App::defer(cx, move |cx| {
            let mut opened = Vec::new();
            for (index, screen) in frames {
                let owner = app.clone();
                let focused = pointer.is_some_and(|pointer| screen.bounds.contains(&pointer));
                let (bounds, display_id) =
                    crate::app::window::popup_frame::place_global(screen.bounds, cx);
                let result = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        display_id,
                        titlebar: None,
                        focus: true,
                        show: true,
                        kind: WindowKind::PopUp,
                        is_movable: false,
                        is_resizable: false,
                        is_minimizable: false,
                        app_id: crate::gpui_platform_window_app_id(),
                        icon: crate::gpui_platform_window_icon(),
                        window_background: WindowBackgroundAppearance::Opaque,
                        ..Default::default()
                    },
                    move |window, cx| {
                        let view = cx.new(|cx| AreaOverlayView {
                            app: owner.clone(),
                            index,
                            focus: cx.focus_handle(),
                            _observe: match owner.upgrade() {
                                Some(app) => cx.observe(&app, |_, _, cx| cx.notify()),
                                None => Subscription::new(|| {}),
                            },
                        });
                        let focus = view.read(cx).focus.clone();
                        focus.focus(window, cx);
                        view
                    },
                );
                if let Ok(handle) = result {
                    let native = handle
                        .update(cx, |_, window, _| {
                            crate::app::helpers::cef_parent_native_view(window)
                                .ok()
                                .map(|view| view as usize)
                        })
                        .ok()
                        .flatten();
                    opened.push((index, handle, native, focused, screen.bounds));
                }
            }
            // The screen under the pointer takes the keyboard last, so Enter and Esc go there.
            opened.sort_by_key(|(_, _, _, focused, _)| *focused);
            for (_, _, native, _, bounds) in &opened {
                if let Some(native) = native {
                    platform::prepare_floating_window(*native, true);
                    // CDXC:GhostexCapture 2026-10-23 WHY:
                    // User: area selection "shows the whole screen shifted down". GPUI opens the
                    // overlay as a titled panel, and macOS keeps a titled window's top below the
                    // menu bar when it is shown, so the frozen screen was drawn from below the
                    // real menu bar (33pt lower on a notched MacBook) and ran off the bottom. Once
                    // the window is borderless macOS leaves its frame alone, so it is put back over
                    // the whole screen, menu bar and notch included, at the pop-up level above it.
                    #[cfg(target_os = "macos")]
                    platform::set_window_frame(*native, *bounds, 1.0);
                }
                #[cfg(not(target_os = "macos"))]
                let _ = bounds;
            }
            let Some(app) = app.upgrade() else {
                return;
            };
            app.update(cx, |app, cx| {
                if let Some(area) = app.ghostex_capture.area.as_mut() {
                    for (index, handle, _, _, _) in opened {
                        if let Some(entry) = area.screens.get_mut(index) {
                            entry.window = Some(handle);
                        }
                    }
                } else {
                    // Cancelled while the windows opened.
                    for (_, handle, _, _, _) in opened {
                        App::defer(cx, move |cx| {
                            let _ = handle.update(cx, |_, window, _| window.remove_window());
                        });
                    }
                }
                cx.notify();
            });
        });
    }

    fn close_ghostex_capture_area(&mut self, cx: &mut Context<Self>) {
        let Some(area) = self.ghostex_capture.area.take() else {
            return;
        };
        for handle in area.screens.into_iter().filter_map(|entry| entry.window) {
            App::defer(cx, move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    fn ghostex_capture_area_down(
        &mut self,
        index: usize,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(area) = self.ghostex_capture.area.as_mut() else {
            return;
        };
        let p = P::new(f32::from(at.x), f32::from(at.y));
        let drag = match area.selection {
            Some((screen, selection)) if screen == index => {
                if let Some(handle) = BOX_HANDLES.iter().copied().find(|handle| {
                    let spot = handle_point(&selection, *handle);
                    (spot.x - p.x).abs() <= HANDLE_SLOP && (spot.y - p.y).abs() <= HANDLE_SLOP
                }) {
                    AreaDrag::Handle {
                        handle,
                        start: selection,
                    }
                } else if selection.contains(p) {
                    AreaDrag::Move {
                        from: p,
                        start: selection,
                    }
                } else {
                    area.selection = Some((index, Area::from_points(p, p)));
                    AreaDrag::New { from: p }
                }
            }
            _ => {
                area.selection = Some((index, Area::from_points(p, p)));
                AreaDrag::New { from: p }
            }
        };
        area.drag = Some(drag);
        cx.notify();
    }

    fn ghostex_capture_area_moved(
        &mut self,
        index: usize,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(area) = self.ghostex_capture.area.as_mut() else {
            return;
        };
        let (Some(drag), Some((screen, _))) = (area.drag, area.selection) else {
            return;
        };
        if screen != index {
            return;
        }
        let Some(entry) = area.screens.get(index) else {
            return;
        };
        let (width, height) = (
            f32::from(entry.screen.bounds.size.width),
            f32::from(entry.screen.bounds.size.height),
        );
        let p = P::new(
            f32::from(at.x).clamp(0.0, width),
            f32::from(at.y).clamp(0.0, height),
        );
        let next = match drag {
            AreaDrag::New { from } => Area::from_points(from, p),
            AreaDrag::Handle { handle, start } => drag_handle(&start, handle, p),
            AreaDrag::Move { from, start } => {
                let dx = (p.x - from.x).clamp(-start.left, width - start.right);
                let dy = (p.y - from.y).clamp(-start.top, height - start.bottom);
                start.translated(dx, dy)
            }
        };
        area.selection = Some((index, next));
        cx.notify();
    }

    fn ghostex_capture_area_up(&mut self, cx: &mut Context<Self>) {
        let Some(area) = self.ghostex_capture.area.as_mut() else {
            return;
        };
        area.drag = None;
        if area
            .selection
            .is_some_and(|(_, selection)| selection.width() < 3.0 || selection.height() < 3.0)
        {
            area.selection = None;
        }
        cx.notify();
    }

    /// Enter: crops the frozen screen to the box and opens the editor.
    fn ghostex_capture_area_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(area) = self.ghostex_capture.area.as_ref() else {
            return;
        };
        let Some((index, selection)) = area.selection else {
            return;
        };
        let Some(entry) = area.screens.get(index) else {
            return;
        };
        let scale = entry.image.width() as f32 / f32::from(entry.screen.bounds.size.width);
        let crop = image::imageops::crop_imm(
            entry.image.as_ref(),
            (selection.left * scale).round() as u32,
            (selection.top * scale).round() as u32,
            (selection.width() * scale).round().max(1.0) as u32,
            (selection.height() * scale).round().max(1.0) as u32,
        )
        .to_image();
        let screen = entry.screen.clone();
        self.ghostex_capture.saved.last_area = Some(super::persistence::SavedArea {
            display: screen.key.clone(),
            left: selection.left,
            top: selection.top,
            right: selection.right,
            bottom: selection.bottom,
        });
        super::persistence::save(&self.ghostex_capture.saved);
        self.close_ghostex_capture_area(cx);
        self.ghostex_capture_captured(crop, scale, screen, cx);
    }

    fn ghostex_capture_area_key(&mut self, index: usize, key: &str, cx: &mut Context<Self>) {
        match key {
            "enter" | "a" => self.ghostex_capture_area_confirm(cx),
            "escape" => {
                self.close_ghostex_capture_area(cx);
                self.finish_ghostex_capture(cx);
            }
            "space" => {
                self.close_ghostex_capture_area(cx);
                self.ghostex_capture.capturing = false;
                self.start_ghostex_capture(CaptureAction::CurrentApp, cx);
            }
            "f" => {
                // The screen already frozen under this overlay is the full-screen capture.
                let Some(entry) = self
                    .ghostex_capture
                    .area
                    .as_mut()
                    .and_then(|area| area.screens.get(index))
                else {
                    return;
                };
                let image = entry.image.as_ref().clone();
                let scale = image.width() as f32 / f32::from(entry.screen.bounds.size.width);
                let screen = entry.screen.clone();
                self.close_ghostex_capture_area(cx);
                self.ghostex_capture_captured(image, scale, screen, cx);
            }
            _ => {}
        }
    }
}

pub(crate) struct AreaOverlayView {
    app: WeakEntity<GhostexGpuiApp>,
    index: usize,
    focus: FocusHandle,
    _observe: Subscription,
}

impl AreaOverlayView {
    fn with_app(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut GhostexGpuiApp, &mut Context<GhostexGpuiApp>),
    ) {
        if let Some(app) = self.app.upgrade() {
            app.update(cx, f);
        }
    }
}

fn shade(bounds: Bounds<Pixels>) -> impl IntoElement {
    div()
        .absolute()
        .left(bounds.origin.x)
        .top(bounds.origin.y)
        .w(bounds.size.width)
        .h(bounds.size.height)
        .bg(rgba(0x00000080))
}

impl Render for AreaOverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let index = self.index;
        let mut root = div()
            .id(("ghostex-capture-area", index))
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .cursor(CursorStyle::Crosshair)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                let key = event.keystroke.key.clone();
                cx.stop_propagation();
                this.with_app(cx, move |app, cx| {
                    app.ghostex_capture_area_key(index, &key, cx)
                });
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    let at = event.position;
                    this.with_app(cx, move |app, cx| {
                        app.ghostex_capture_area_down(index, at, cx)
                    });
                }),
            )
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    let at = event.position;
                    this.with_app(cx, move |app, cx| {
                        app.ghostex_capture_area_moved(index, at, cx)
                    });
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.with_app(cx, |app, cx| app.ghostex_capture_area_up(cx));
                }),
            );
        let Some(app) = self.app.upgrade() else {
            return root;
        };
        let app = app.read(cx);
        let Some(area) = app.ghostex_capture.area.as_ref() else {
            return root;
        };
        let Some(entry) = area.screens.get(index) else {
            return root;
        };
        let viewport = window.viewport_size();
        root = root.child(
            img(ImageSource::Render(entry.render.clone()))
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(viewport.width)
                .h(viewport.height),
        );
        let selection = area
            .selection
            .filter(|(screen, _)| *screen == index)
            .map(|(_, selection)| selection);
        match selection {
            None => {
                root = root.child(shade(Bounds::new(point(px(0.0), px(0.0)), viewport)));
            }
            Some(selection) => {
                let (left, top, right, bottom) = (
                    px(selection.left),
                    px(selection.top),
                    px(selection.right),
                    px(selection.bottom),
                );
                root = root
                    .child(shade(Bounds::new(
                        point(px(0.0), px(0.0)),
                        size(viewport.width, top),
                    )))
                    .child(shade(Bounds::new(
                        point(px(0.0), bottom),
                        size(viewport.width, viewport.height - bottom),
                    )))
                    .child(shade(Bounds::new(
                        point(px(0.0), top),
                        size(left, bottom - top),
                    )))
                    .child(shade(Bounds::new(
                        point(right, top),
                        size(viewport.width - right, bottom - top),
                    )))
                    .child(
                        div()
                            .absolute()
                            .left(left)
                            .top(top)
                            .w(right - left)
                            .h(bottom - top)
                            .border_1()
                            .border_dashed()
                            .border_color(rgb(0xffffff)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(left)
                            .top((top - px(24.0)).max(px(4.0)))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(5.0))
                            .bg(rgb(0x111111))
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(0xe8e8e8))
                            .child(format!(
                                "{} × {}",
                                selection.width().round(),
                                selection.height().round()
                            )),
                    );
                for handle in BOX_HANDLES {
                    let spot = handle_point(&selection, handle);
                    root = root.child(
                        div()
                            .absolute()
                            .left(px(spot.x - 4.0))
                            .top(px(spot.y - 4.0))
                            .size(px(8.0))
                            .rounded(px(2.0))
                            .bg(rgb(0xffffff)),
                    );
                }
            }
        }
        root.child(
            div()
                .absolute()
                .bottom(px(28.0))
                .left(px(0.0))
                .right(px(0.0))
                .flex()
                .justify_center()
                .child(
                    div()
                        .px(px(14.0))
                        .py(px(8.0))
                        .rounded(px(10.0))
                        .bg(rgba(0x141414eb))
                        .border_1()
                        .border_color(rgba(0xffffff29))
                        .text_size(px(12.5))
                        .text_color(rgb(0xe8e8e8))
                        .child(
                            "Drag outside the box to select another area · Enter or A capture · Space current app · F full screen · Esc cancel",
                        ),
                ),
        )
    }
}
