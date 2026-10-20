#[cfg(target_os = "windows")]
use std::io;

use serde_json::{Value, json};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};
use wry::{WebView, WebViewBuilder};

use crate::*;

impl EditorApp {
    pub(crate) fn ensure_warm_window(&mut self, target: &EventLoopWindowTarget<DaemonEvent>) {
        if self.pending_shutdown || self.warm_window.is_some() {
            return;
        }
        match self.make_editor_window(target) {
            Ok(window_id) => self.warm_window = Some(window_id),
            Err(error) => eprintln!("ghostex-editor: unable to warm editor window: {error}"),
        }
    }

    pub(crate) fn warm_window_is_ready(&self) -> bool {
        let Some(window_id) = self.warm_window else {
            return false;
        };
        self.windows
            .get(&window_id)
            .is_some_and(|window| window.is_ready && window.session_request_id.is_none())
    }

    pub(crate) fn notify_warm_waiters_if_ready(&mut self, window_id: WindowId) {
        if Some(window_id) != self.warm_window || !self.warm_window_is_ready() {
            return;
        }
        let waiters = std::mem::take(&mut self.warm_waiters);
        for waiter in waiters {
            waiter.send(json!({"type": "warmed", "v": PROTOCOL_VERSION}));
        }
    }

    pub(crate) fn take_ready_warm_window(&mut self) -> Option<WindowId> {
        if !self.warm_window_is_ready() {
            return None;
        }
        self.warm_window.take()
    }

    pub(crate) fn make_editor_window(
        &mut self,
        target: &EventLoopWindowTarget<DaemonEvent>,
    ) -> Result<WindowId, String> {
        let builder = apply_window_platform_policy(
            WindowBuilder::new()
                .with_title(APP_WINDOW_TITLE)
                .with_inner_size(LogicalSize::new(900.0, 620.0))
                .with_min_inner_size(LogicalSize::new(480.0, 320.0))
                .with_visible(false),
        );
        let window = builder
            .build(target)
            .map_err(|error| format!("unable to create window: {error}"))?;
        #[cfg(target_os = "windows")]
        hide_windows_titlebar_icon(&window)?;
        let window_id = window.id();
        let proxy = self.proxy.clone();
        let web_root = self.web_root.clone();
        let protocol_root = self.web_root.clone();
        #[cfg(target_os = "windows")]
        let webview_builder = WebViewBuilder::new_with_web_context(&mut self.web_context);
        #[cfg(not(target_os = "windows"))]
        let webview_builder = WebViewBuilder::new();
        let webview_builder = webview_builder
            .with_initialization_script(
                r#"
Object.defineProperty(window, "__require", {
  configurable: true,
  get: function() { return window.require; }
});
"#,
            )
            .with_custom_protocol(CUSTOM_PROTOCOL.to_string(), move |_webview_id, request| {
                asset_response(&protocol_root, request)
            })
            .with_ipc_handler(move |request| {
                let _ = proxy.send_event(DaemonEvent::WebMessage {
                    window_id,
                    body: request.body().clone(),
                });
            })
            .with_url(format!("{CUSTOM_PROTOCOL}://localhost/index.html"));
        let webview = build_webview(webview_builder, &window)
            .map_err(|error| format!("unable to create webview: {error}"))?;
        self.windows.insert(
            window_id,
            EditorWindow {
                window,
                webview,
                is_ready: false,
                session_request_id: None,
            },
        );
        if !web_root.join("index.html").is_file() {
            return Err("editor web root disappeared".to_string());
        }
        Ok(window_id)
    }

    pub(crate) fn configure_window_if_ready(&mut self, window_id: WindowId) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        let Some(window) = self.windows.get(&window_id) else {
            return;
        };
        if !window.is_ready {
            return;
        }
        let Some(session) = self.sessions.get(&request_id) else {
            return;
        };
        let mut detail = json!({
            "type": "configure",
            "initialText": session.initial_text,
            "language": session.language,
            "filePath": session.file_path,
            "title": session.title,
        });
        if let Some(cursor_offset) = session.initial_cursor_offset {
            detail["cursorOffset"] = json!(cursor_offset);
        }
        dispatch_host_message(window, &detail);
    }

    pub(crate) fn session_configured(&mut self, window_id: WindowId) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        let Some((status_file, opener, opened_request_id)) =
            self.sessions.get_mut(&request_id).and_then(|session| {
                if session.has_opened {
                    return None;
                }
                session.has_opened = true;
                Some((
                    session.status_file.clone(),
                    session.opener.clone(),
                    session.request_id.clone(),
                ))
            })
        else {
            return;
        };
        write_status(&status_file, "started");
        opener.send(json!({
            "type": "opened",
            "requestId": opened_request_id,
        }));
    }

    pub(crate) fn present_window(&mut self, window_id: WindowId) {
        let saved_frame = load_saved_window_frame();
        let offset = self.cascade_offset;
        if saved_frame.is_none() {
            self.cascade_offset = (self.cascade_offset + 28) % 224;
        }
        if let Some(editor_window) = self.windows.get(&window_id) {
            if let Some(frame) = saved_frame {
                editor_window
                    .window
                    .set_inner_size(LogicalSize::new(frame.width, frame.height));
                editor_window
                    .window
                    .set_outer_position(LogicalPosition::new(frame.x, frame.y));
            } else {
                editor_window
                    .window
                    .set_outer_position(LogicalPosition::new(
                        80.0 + offset as f64,
                        80.0 + offset as f64,
                    ));
            }
            editor_window.window.set_visible(true);
            editor_window.window.set_focus();
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn apply_window_platform_policy(builder: WindowBuilder) -> WindowBuilder {
    apply_skip_taskbar(builder)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn apply_window_platform_policy(builder: WindowBuilder) -> WindowBuilder {
    builder
}

#[cfg(target_os = "linux")]
fn apply_skip_taskbar(builder: WindowBuilder) -> WindowBuilder {
    use tao::platform::unix::WindowBuilderExtUnix;
    builder.with_skip_taskbar(true)
}

#[cfg(target_os = "windows")]
fn apply_skip_taskbar(builder: WindowBuilder) -> WindowBuilder {
    use tao::platform::windows::WindowBuilderExtWindows;
    builder.with_skip_taskbar(true)
}

/// CDXC:PromptEditor 2026-09-16 DECISION:
/// User: hide the Windows prompt editor's titlebar icon, superseding the earlier request for a better icon.
/// The dialog frame suppresses Windows' generic icon when no window icons are assigned.
#[cfg(target_os = "windows")]
fn hide_windows_titlebar_icon(window: &Window) -> Result<(), String> {
    use tao::platform::windows::WindowExtWindows;
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WS_EX_DLGMODALFRAME,
    };

    // SAFETY: this newly created HWND belongs to the current UI thread and remains alive throughout.
    unsafe {
        let hwnd = window.hwnd() as _;
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetLastError(0);
        if SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_DLGMODALFRAME as isize) == 0
            && GetLastError() != 0
        {
            return Err(format!(
                "unable to hide editor titlebar icon: {}",
                io::Error::last_os_error()
            ));
        }
        if SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        ) == 0
        {
            return Err(format!(
                "unable to update editor titlebar: {}",
                io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> wry::Result<WebView> {
    builder.build(window)
}

#[cfg(target_os = "linux")]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> wry::Result<WebView> {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    let vbox = window
        .default_vbox()
        .ok_or_else(|| wry::Error::Message("missing GTK vbox".into()))?;
    builder.build_gtk(vbox)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> wry::Result<WebView> {
    builder.build(window)
}

pub(crate) fn dispatch_host_message(window: &EditorWindow, detail: &Value) {
    let Ok(json) = serde_json::to_string(detail) else {
        return;
    };
    let script = format!(
        "window.dispatchEvent(new CustomEvent(\"ghostex-editor-host-message\", {{ detail: {json} }}));"
    );
    let _ = window.webview.evaluate_script(&script);
}
