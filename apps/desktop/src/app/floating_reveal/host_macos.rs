//! The AppKit host: the panel is a real child window of the main window, and AppKit runs the slide
//! and the pointer-leave dismissal inside `native/macos/GpuiSidebarReveal.m`.

use std::ffi::c_void;

use gpui::Pixels;
use gpui::Point;

use super::model::*;
use crate::app::render::workarea_header::workarea_header_bottom_y;
use crate::*;

unsafe extern "C" {
    fn GhostexGpuiNativeSidebarRevealRequest(
        root: *mut c_void,
        width: f64,
        titlebar_height: f64,
        left_inset: f64,
        edge_hovered: bool,
        requested: bool,
        keep_under_pointer: bool,
    ) -> i32;
    fn GhostexGpuiNativeSidebarRevealUpdate(
        root: *mut c_void,
        popup: *mut c_void,
        enabled: bool,
        width: f64,
        titlebar_height: f64,
        left_inset: f64,
        requested: bool,
        sticky: bool,
        slide_seconds: f64,
    ) -> bool;
    fn GhostexGpuiNativeSidebarRevealLeaving(popup: *mut c_void) -> bool;
    fn GhostexGpuiRevealEdgeSync(root: *mut c_void, visible: bool, width: f64);
    fn GhostexGpuiRevealEdgeTake(root: *mut c_void, y_fraction: *mut f64) -> i32;
}

impl GhostexGpuiApp {
    /// Keeps the native hot zone over the window's left edge while the sidebar is collapsed and
    /// hands what the pointer did there since the last sweep to the same arm and disarm the GPUI zone
    /// calls on the other platforms.
    pub(super) fn sync_floating_reveal_native_edge(&mut self) {
        let visible = self.floating_reveal_edge_strip_visible();
        unsafe {
            GhostexGpuiRevealEdgeSync(
                self.parent_ns_view,
                visible,
                FLOATING_REVEAL_EDGE_WIDTH as f64,
            )
        };
        if !visible {
            return;
        }
        let mut y_fraction = 0.0;
        match unsafe { GhostexGpuiRevealEdgeTake(self.parent_ns_view, &mut y_fraction) } {
            1 => {
                let want = self.floating_reveal_edge_want(y_fraction < 0.5);
                self.arm_floating_reveal_from_strip(want);
            }
            2 => self.disarm_floating_reveal_edge(),
            _ => {}
        }
    }

    pub(super) fn sync_floating_reveal_host(
        &mut self,
        requested: bool,
        keep_under_pointer: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(panel) = self.floating_reveal.panel.as_ref() {
            let visible = unsafe {
                GhostexGpuiNativeSidebarRevealUpdate(
                    self.parent_ns_view,
                    panel.native_view,
                    true,
                    panel.width as f64,
                    workarea_header_bottom_y() as f64,
                    self.floating_reveal_left_inset() as f64,
                    requested,
                    false,
                    floating_reveal_slide_duration(cx.reduce_motion()).as_secs_f64(),
                )
            };
            if !visible {
                self.close_floating_reveal(cx);
            } else if unsafe { GhostexGpuiNativeSidebarRevealLeaving(panel.native_view) } {
                self.dismiss_floating_reveal_chat_windows(cx);
            }
            return;
        }
        let content = self.floating_reveal_content();
        let width = self.floating_reveal_width_for(content);
        let request = unsafe {
            GhostexGpuiNativeSidebarRevealRequest(
                self.parent_ns_view,
                width as f64,
                workarea_header_bottom_y() as f64,
                self.floating_reveal_left_inset() as f64,
                self.floating_reveal.edge_hovered,
                requested,
                keep_under_pointer,
            )
        };
        if request == 1 {
            self.open_floating_reveal(keep_under_pointer, cx);
        }
    }

    pub(super) fn attach_floating_reveal_panel(
        &mut self,
        window: gpui::WindowHandle<gpui_component::Root>,
        native_view: Option<*mut c_void>,
        anchor: Point<Pixels>,
        content: FloatingRevealContent,
        width: f32,
        sticky: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(native_view) = native_view else {
            let _ = window.update(cx, |_, window, _| window.remove_window());
            return false;
        };
        let requested = self.floating_reveal.requested_until.is_some();
        self.floating_reveal.panel = Some(FloatingRevealPanel {
            window,
            native_view,
            anchor,
            content,
            width,
        });
        // A links card opened from the docked sidebar does not stay up over the floating one.
        if content.sidebar {
            self.close_session_links_card(cx);
        }
        let visible = unsafe {
            GhostexGpuiNativeSidebarRevealUpdate(
                self.parent_ns_view,
                native_view,
                true,
                width as f64,
                workarea_header_bottom_y() as f64,
                self.floating_reveal_left_inset() as f64,
                requested,
                sticky,
                floating_reveal_slide_duration(cx.reduce_motion()).as_secs_f64(),
            )
        };
        if !visible {
            self.close_floating_reveal(cx);
            return false;
        }
        // AppKit sized the panel from the frame it computed just now, from inside this update;
        // GPUI has to be told about that size the same way it is told about every other one.
        self.schedule_floating_reveal_bounds_refresh(cx);
        true
    }

    pub(super) fn dispose_floating_reveal_host(&self, panel: &FloatingRevealPanel) {
        unsafe {
            GhostexGpuiNativeSidebarRevealUpdate(
                std::ptr::null_mut(),
                panel.native_view,
                false,
                0.0,
                0.0,
                0.0,
                false,
                false,
                0.0,
            );
        }
    }
}
