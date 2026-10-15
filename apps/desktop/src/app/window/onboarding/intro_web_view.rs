//! The operating system's own web view that plays the first-run intro video: WKWebView through the
//! native/macos/GpuiIntroVideoWebView.m shim, WebView2 through wry on Windows. It is a real child
//! view of the onboarding window, placed each frame at the exact bounds the page lays out for it,
//! and it is removed when this value drops. Linux builds have none (`SUPPORTED` is false).
//! SEE-ALSO: apps/desktop/src/app/window/onboarding/intro_video.rs (the page and the CDXC:Onboarding 2026-10-01 decision).
use gpui::{Bounds, Pixels, Window};
use std::cell::Cell;

pub(crate) struct IntroWebView {
    backend: backend::Backend,
    frame: Cell<Option<Bounds<Pixels>>>,
}

impl IntroWebView {
    pub(crate) const SUPPORTED: bool = backend::SUPPORTED;

    /// Starts showing `html` as a page with a real https origin (so the YouTube iframe in it sends
    /// a Referer), hidden until the first `place`.
    pub(crate) fn new(window: &Window, html: &str) -> Result<Self, String> {
        Ok(Self {
            backend: backend::Backend::new(window, html)?,
            frame: Cell::new(None),
        })
    }

    /// Moves the view onto `bounds` (window coordinates, logical pixels); unchanged bounds cost nothing.
    pub(crate) fn place(&self, bounds: Bounds<Pixels>) {
        if self.frame.get() == Some(bounds) {
            return;
        }
        self.frame.set(Some(bounds));
        self.backend.set_frame(
            f64::from(bounds.origin.x.as_f32()),
            f64::from(bounds.origin.y.as_f32()),
            f64::from(bounds.size.width.as_f32()),
            f64::from(bounds.size.height.as_f32()),
        );
    }

    /// Links the player asked to open in a new window, for the host to open in the default browser
    /// (the macOS shim opens them itself).
    pub(crate) fn take_opened_links(&self) -> Vec<String> {
        self.backend.take_opened_links()
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::{CString, c_char, c_void};

    pub(super) const SUPPORTED: bool = true;

    /// The page's origin: WKWebView takes any https base URL for an HTML string.
    const PAGE_BASE_URL: &str = "https://ghostex.dev/";

    unsafe extern "C" {
        fn GhostexGpuiIntroVideoWebViewCreate(
            parent_view: *mut c_void,
            html: *const c_char,
            base_url: *const c_char,
        ) -> *mut c_void;
        fn GhostexGpuiIntroVideoWebViewSetFrame(
            handle: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        );
        fn GhostexGpuiIntroVideoWebViewDestroy(handle: *mut c_void);
    }

    pub(super) struct Backend {
        handle: *mut c_void,
    }

    impl Backend {
        pub(super) fn new(window: &gpui::Window, html: &str) -> Result<Self, String> {
            // gpui's own `Window::window_handle` is its app-level handle, so the trait is named.
            let parent = match HasWindowHandle::window_handle(window)
                .map_err(|error| error.to_string())?
                .as_raw()
            {
                RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
                _ => return Err("the window has no AppKit view".to_string()),
            };
            let html = CString::new(html).map_err(|error| error.to_string())?;
            let base_url = CString::new(PAGE_BASE_URL).map_err(|error| error.to_string())?;
            let handle = unsafe {
                GhostexGpuiIntroVideoWebViewCreate(parent, html.as_ptr(), base_url.as_ptr())
            };
            if handle.is_null() {
                return Err("WKWebView could not be created".to_string());
            }
            Ok(Self { handle })
        }

        pub(super) fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
            unsafe { GhostexGpuiIntroVideoWebViewSetFrame(self.handle, x, y, width, height) };
        }

        pub(super) fn take_opened_links(&self) -> Vec<String> {
            Vec::new()
        }
    }

    impl Drop for Backend {
        fn drop(&mut self) {
            unsafe { GhostexGpuiIntroVideoWebViewDestroy(self.handle) };
        }
    }
}

#[cfg(target_os = "windows")]
mod backend {
    use std::borrow::Cow;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wry::WebViewBuilderExtWindows as _;
    use wry::dpi::{LogicalPosition, LogicalSize};
    use wry::http::{HeaderValue, Response, header::CONTENT_TYPE};

    pub(super) const SUPPORTED: bool = true;

    /// wry serves this custom protocol at `https://ghostex.localhost/`, a secure origin YouTube
    /// accepts as the embedding site (NavigateToString pages are `about:blank`, which it refuses).
    const PAGE_PROTOCOL: &str = "ghostex";

    /// CDXC:Onboarding 2026-10-07 WHY:
    /// Without its own user data folder WebView2 keeps its profile next to the executable (`Ghostex.exe.WebView2`), which a machine-wide install under `C:\Program Files\Ghostex` cannot write: creation fails with E_ACCESSDENIED and the intro falls back to the "Watch the intro on YouTube" still. The profile is a cache, so it lives in Ghostex's per-user cache folder beside CEF's (the GhostexEditor helper has the same rule for its own WebView2).
    fn data_directory() -> std::path::PathBuf {
        ghostex_paths::GhostexPaths::resolve()
            .cache_dir
            .join("intro-webview2")
    }

    pub(super) struct Backend {
        web_view: wry::WebView,
        opened_links: Rc<RefCell<Vec<String>>>,
        /// Kept for the web view's lifetime; WebView2 reads it while it creates its environment.
        _web_context: wry::WebContext,
    }

    impl Backend {
        pub(super) fn new(window: &gpui::Window, html: &str) -> Result<Self, String> {
            let page: Cow<'static, [u8]> = Cow::Owned(html.as_bytes().to_vec());
            let opened_links = Rc::new(RefCell::new(Vec::new()));
            let queue = opened_links.clone();
            let mut web_context = wry::WebContext::new(Some(data_directory()));
            let web_view = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                .with_https_scheme(true)
                .with_custom_protocol(PAGE_PROTOCOL.to_string(), move |_, _| {
                    let mut response = Response::new(page.clone());
                    response.headers_mut().insert(
                        CONTENT_TYPE,
                        HeaderValue::from_static("text/html; charset=utf-8"),
                    );
                    response
                })
                .with_url(format!("{PAGE_PROTOCOL}://localhost/"))
                // Made only when the user pressed Play on the page's still, so the player starts at once.
                .with_autoplay(true)
                .with_visible(false)
                .with_focused(false)
                .with_background_color((0, 0, 0, 255))
                .with_new_window_req_handler(move |url, _features| {
                    queue.borrow_mut().push(url);
                    wry::NewWindowResponse::Deny
                })
                .build_as_child(window)
                .map_err(|error| error.to_string())?;
            Ok(Self {
                web_view,
                opened_links,
                _web_context: web_context,
            })
        }

        pub(super) fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
            let _ = self.web_view.set_bounds(wry::Rect {
                position: LogicalPosition::new(x, y).into(),
                size: LogicalSize::new(width.max(0.0), height.max(0.0)).into(),
            });
            let _ = self.web_view.set_visible(true);
        }

        pub(super) fn take_opened_links(&self) -> Vec<String> {
            std::mem::take(&mut *self.opened_links.borrow_mut())
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod backend {
    pub(super) const SUPPORTED: bool = false;

    pub(super) struct Backend;

    impl Backend {
        pub(super) fn new(_window: &gpui::Window, _html: &str) -> Result<Self, String> {
            Err("this platform has no system web view".to_string())
        }

        pub(super) fn set_frame(&self, _x: f64, _y: f64, _width: f64, _height: f64) {}

        pub(super) fn take_opened_links(&self) -> Vec<String> {
            Vec::new()
        }
    }
}
