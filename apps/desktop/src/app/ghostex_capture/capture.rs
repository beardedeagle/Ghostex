//! Taking the screenshot for an action and handing it to the editor, or opening the prompt box.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: "after clicking take a screenshot we need to hide the whole ghostex capture ui". The
//! button, the prompt box and any open editor are moved off screen before the pixels are read and
//! come back once the capture ends; "current app" is the app the user was in before calling up
//! Ghostex Capture, captured straight away with no picking step.

use std::time::Duration;

use gpui::{Bounds, Context, point, px};
use image::RgbaImage;

use super::grab;
use super::model::CaptureAction;
use super::placement::{self, Screen};
use super::platform;
use super::save;
use crate::GhostexGpuiApp;

/// Long enough for the window server to redraw without our windows before reading pixels.
const HIDE_SETTLE: Duration = Duration::from_millis(140);

fn off_screen(frame: Bounds<gpui::Pixels>) -> Bounds<gpui::Pixels> {
    Bounds::new(point(px(-30000.0), px(-30000.0)), frame.size)
}

impl GhostexGpuiApp {
    pub(super) fn start_ghostex_capture(&mut self, action: CaptureAction, cx: &mut Context<Self>) {
        match action {
            CaptureAction::Prompt => {
                self.start_fresh_ghostex_capture_prompt(cx);
                return;
            }
            CaptureAction::ContinuePrompt => {
                self.open_ghostex_capture_prompt(cx);
                return;
            }
            _ => {}
        }
        if self.ghostex_capture.capturing {
            return;
        }
        if !grab::screen_access(false) {
            self.ask_ghostex_capture_screen_access(cx);
            return;
        }
        // A picture still in the editor goes to the prompt before the next one is taken.
        self.commit_open_ghostex_capture_editor(cx);
        self.ghostex_capture.capturing = true;
        self.hide_ghostex_capture_windows(cx);
        let scale = self
            .ghostex_capture
            .icon
            .as_ref()
            .map(|icon| icon.scale)
            .unwrap_or(1.0);
        let frontmost = self.ghostex_capture.frontmost;
        let pointer = platform::pointer(scale);
        let screens = placement::screens(cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HIDE_SETTLE).await;
            match action {
                CaptureAction::Area => {
                    let grabs = cx
                        .background_executor()
                        .spawn(async move {
                            screens
                                .into_iter()
                                .filter_map(|screen| {
                                    grab::grab_area(screen.bounds, scale).map(|image| (screen, image))
                                })
                                .collect::<Vec<_>>()
                        })
                        .await;
                    let _ = this.update(cx, |app, cx| app.open_ghostex_capture_area(grabs, cx));
                }
                CaptureAction::FullScreen => {
                    let screen = pointer
                        .and_then(|pointer| {
                            screens
                                .iter()
                                .find(|screen| screen.bounds.contains(&pointer))
                                .cloned()
                        })
                        .or_else(|| screens.first().cloned());
                    let grabbed = cx
                        .background_executor()
                        .spawn(async move {
                            let screen = screen?;
                            let image = grab::grab_area(screen.bounds, scale)?;
                            Some((screen, image))
                        })
                        .await;
                    let _ = this.update(cx, |app, cx| match grabbed {
                        Some((screen, image)) => {
                            let px_per_pt =
                                image.width() as f32 / f32::from(screen.bounds.size.width);
                            app.ghostex_capture_captured(image, px_per_pt, screen, cx);
                        }
                        None => app.ghostex_capture_failed("Could not capture the screen.", cx),
                    });
                }
                CaptureAction::CurrentApp => {
                    let grabbed = cx
                        .background_executor()
                        .spawn(async move { grab::grab_app(frontmost, scale) })
                        .await;
                    let _ = this.update(cx, |app, cx| match grabbed {
                        Some(window) => {
                            let screen = placement::screen_at(window.frame.center(), cx)
                                .or_else(|| placement::screens(cx).into_iter().next());
                            let Some(screen) = screen else {
                                app.ghostex_capture_failed("Could not find the screen.", cx);
                                return;
                            };
                            let px_per_pt = window.image.width() as f32
                                / f32::from(window.frame.size.width).max(1.0);
                            app.ghostex_capture_captured(window.image, px_per_pt, screen, cx);
                        }
                        None => app.ghostex_capture_failed(
                            "Could not capture the app you were using. Try Screenshot an area instead.",
                            cx,
                        ),
                    });
                }
                CaptureAction::TogglePanel
                | CaptureAction::Prompt
                | CaptureAction::ContinuePrompt => {}
            }
        })
        .detach();
    }

    /// A capture came back: save it as taken and open the editor on it.
    pub(super) fn ghostex_capture_captured(
        &mut self,
        image: RgbaImage,
        px_per_pt: f32,
        screen: Screen,
        cx: &mut Context<Self>,
    ) {
        let original = match save::save_png(&image, false) {
            Ok(path) => path,
            Err(message) => {
                self.ghostex_capture_failed(&message, cx);
                return;
            }
        };
        self.finish_ghostex_capture(cx);
        self.open_ghostex_capture_editor(image, original, px_per_pt, screen, cx);
    }

    pub(super) fn ghostex_capture_failed(&mut self, message: &str, cx: &mut Context<Self>) {
        self.finish_ghostex_capture(cx);
        self.dispatch_gpui_app_modal_toast("warning", "Floating Capture", message, cx);
    }

    /// Brings Ghostex Capture's windows back after a capture ends or is cancelled.
    pub(super) fn finish_ghostex_capture(&mut self, cx: &mut Context<Self>) {
        if !self.ghostex_capture.capturing {
            return;
        }
        self.ghostex_capture.capturing = false;
        if let Some(icon) = self.ghostex_capture.icon.as_mut() {
            // Forces the next sync to place the window again.
            icon.frame = Bounds::default();
        }
        self.sync_ghostex_capture_icon(cx);
        self.show_ghostex_capture_prompt_window();
    }

    fn hide_ghostex_capture_windows(&mut self, cx: &mut gpui::App) {
        if let Some(icon) = self.ghostex_capture.icon.as_ref() {
            platform::set_window_frame(icon.native, off_screen(icon.frame), icon.scale);
        }
        self.hide_ghostex_capture_prompt_window(cx);
    }
}
