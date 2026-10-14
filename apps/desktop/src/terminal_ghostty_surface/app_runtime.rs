use std::{
    ffi::{CStr, c_char, c_void},
    ptr::{self, NonNull},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

use crate::ghostty_kit::ffi;

use super::*;

struct GhosttyRuntimeCallbackState {
    app: AtomicPtr<c_void>,
    wakeup_requested: AtomicBool,
}

impl GhosttyRuntimeCallbackState {
    fn new() -> Self {
        Self {
            app: AtomicPtr::new(ptr::null_mut()),
            wakeup_requested: AtomicBool::new(false),
        }
    }

    fn mark_app_ready(&self, app: ffi::ghostty_app_t) {
        self.app.store(app, Ordering::SeqCst);
    }
}

pub(crate) struct GhosttyAppOwner {
    app: NonNull<c_void>,
    #[allow(dead_code)]
    // ownership handle: held so the ghostty app keeps its config/runtime-config alive for the C side, never read from Rust
    config: GhosttyConfigOwner,
    runtime_state: Box<GhosttyRuntimeCallbackState>,
    #[allow(dead_code)]
    // ownership handle: held so the ghostty app keeps its config/runtime-config alive for the C side, never read from Rust
    runtime_config: ffi::ghostty_runtime_config_s,
    pub(crate) functions: GhosttyKitFunctionTable,
    latest_focus_state: Option<bool>,
}

impl GhosttyAppOwner {
    #[cfg(target_os = "macos")]
    pub(crate) fn new() -> Result<Self, GhosttySurfaceRuntimeError> {
        let functions = GhosttyKitFunctionTable::production();
        initialize_production_ghostty_once(functions)?;
        Self::new_after_runtime_init(functions)
    }

    fn new_after_runtime_init(
        functions: GhosttyKitFunctionTable,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        let config =
            GhosttyConfigOwner::load_ghostex_terminal_config_finalized_with_functions(functions)?;
        let runtime_state = Box::new(GhosttyRuntimeCallbackState::new());
        let runtime_config = runtime_config_for_state(&runtime_state);
        let app = unsafe { (functions.app_new)(&runtime_config, config.as_raw()) };
        let app = NonNull::new(app).ok_or(GhosttySurfaceRuntimeError::AppCreateReturnedNull)?;
        runtime_state.mark_app_ready(app.as_ptr());

        Ok(Self {
            app,
            config,
            runtime_state,
            runtime_config,
            functions,
            latest_focus_state: None,
        })
    }

    pub(crate) fn as_raw(&self) -> ffi::ghostty_app_t {
        self.app.as_ptr()
    }

    pub(crate) fn tick(&self) {
        unsafe {
            (self.functions.app_tick)(self.as_raw());
        }
    }

    pub(crate) fn tick_if_woken(&self) {
        if self
            .runtime_state
            .wakeup_requested
            .swap(false, Ordering::SeqCst)
        {
            self.tick();
        }
    }

    pub(crate) fn set_focus(&mut self, focused: bool) {
        if self.latest_focus_state == Some(focused) {
            return;
        }
        unsafe {
            (self.functions.app_set_focus)(self.as_raw(), focused);
        }
        self.latest_focus_state = Some(focused);
    }
}

impl Drop for GhosttyAppOwner {
    fn drop(&mut self) {
        self.runtime_state
            .app
            .store(ptr::null_mut(), Ordering::SeqCst);
        unsafe {
            (self.functions.app_free)(self.as_raw());
        }
    }
}

/*
CDXC:Clipboard 2026-06-27-04:10:
Ghostty runtime clipboard callbacks must never touch GPUI App clipboard APIs directly. Embedded Ghostty passes surface userdata to clipboard callbacks, so GPUI may accept only registered surface close tokens with mounted surfaces, enqueue owner-local standard-clipboard operations, and let the app-thread drain perform explicit clipboard access for the exact Agents or command surface owner.

CDXC:Clipboard 2026-06-27-04:10:
The low-level Ghostty clipboard path is surface-scoped only. Runtime app userdata, null request state, selection clipboard requests, missing `text/plain` write payloads, or focused-surface inference must not authorize clipboard access. Initial reads complete as unconfirmed, confirm callbacks borrow the original content pointer only for synchronous completion, and callbacks must not log, persist, or retain raw clipboard diagnostics beyond the owner-local queue.
*/
const GHOSTTY_RUNTIME_SUPPORTS_SELECTION_CLIPBOARD: bool = false;
pub(crate) const GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_MIME: &[u8] = b"text/plain";
pub(crate) const GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_C_STRING: &[u8] = b"text/plain\0";

fn runtime_config_for_state(state: &GhosttyRuntimeCallbackState) -> ffi::ghostty_runtime_config_s {
    ffi::ghostty_runtime_config_s {
        userdata: state as *const GhosttyRuntimeCallbackState as *mut c_void,
        supports_selection_clipboard: GHOSTTY_RUNTIME_SUPPORTS_SELECTION_CLIPBOARD,
        wakeup_cb: Some(ghostty_runtime_wakeup_cb),
        action_cb: Some(ghostty_runtime_action_cb),
        read_clipboard_cb: Some(ghostty_runtime_read_clipboard_cb),
        confirm_read_clipboard_cb: Some(ghostty_runtime_confirm_read_clipboard_cb),
        write_clipboard_cb: Some(ghostty_runtime_write_clipboard_cb),
        close_surface_cb: Some(ghostty_runtime_close_surface_cb),
    }
}

unsafe extern "C" fn ghostty_runtime_wakeup_cb(userdata: *mut c_void) {
    let Some(state) = NonNull::new(userdata as *mut GhosttyRuntimeCallbackState) else {
        return;
    };
    unsafe {
        state
            .as_ref()
            .wakeup_requested
            .store(true, Ordering::SeqCst);
    }
}

/// Dispatches Ghostty runtime actions to the owning surface's app-thread
/// queue. Only surface-targeted, product-relevant tags are handled; returning
/// false leaves the remaining tags to Ghostty's default behavior, matching the
/// macOS host's dispatcher in TerminalWorkspaceView.
unsafe extern "C" fn ghostty_runtime_action_cb(
    _app: ffi::ghostty_app_t,
    target: ffi::ghostty_target_s,
    action: ffi::ghostty_action_s,
) -> bool {
    let Some(event) = (unsafe { runtime_action_event_from_action(action) }) else {
        return false;
    };
    let Some(token) = registered_surface_close_token_for_action_target(target) else {
        return false;
    };
    unsafe {
        token.as_ref().enqueue_runtime_action_event(event);
    }
    true
}

unsafe fn runtime_action_event_from_action(
    action: ffi::ghostty_action_s,
) -> Option<GhosttyRuntimeActionEvent> {
    match action.tag {
        ffi::GHOSTTY_ACTION_OPEN_URL => {
            let open_url = unsafe { action.action.open_url };
            let url = unsafe { runtime_action_sized_string(open_url.url, open_url.len) }?;
            Some(GhosttyRuntimeActionEvent::OpenUrl { url })
        }
        ffi::GHOSTTY_ACTION_RING_BELL => Some(GhosttyRuntimeActionEvent::RingBell),
        ffi::GHOSTTY_ACTION_SET_TITLE => {
            let title = unsafe { runtime_action_c_string(action.action.set_title.title) }?;
            Some(GhosttyRuntimeActionEvent::SetTitle { title })
        }
        ffi::GHOSTTY_ACTION_PWD => {
            let pwd = unsafe { runtime_action_c_string(action.action.pwd.pwd) }?;
            Some(GhosttyRuntimeActionEvent::Pwd { pwd })
        }
        ffi::GHOSTTY_ACTION_MOUSE_OVER_LINK => {
            let link = unsafe { action.action.mouse_over_link };
            let url = unsafe { runtime_action_sized_string(link.url, link.len) };
            Some(GhosttyRuntimeActionEvent::MouseOverLink { url })
        }
        ffi::GHOSTTY_ACTION_START_SEARCH => {
            let needle = unsafe { runtime_action_c_string(action.action.start_search.needle) };
            Some(GhosttyRuntimeActionEvent::StartSearch { needle })
        }
        ffi::GHOSTTY_ACTION_END_SEARCH => Some(GhosttyRuntimeActionEvent::EndSearch),
        ffi::GHOSTTY_ACTION_SEARCH_TOTAL => {
            let total = unsafe { action.action.search_total.total };
            Some(GhosttyRuntimeActionEvent::SearchTotal {
                total: (total >= 0).then_some(total as u64),
            })
        }
        ffi::GHOSTTY_ACTION_SEARCH_SELECTED => {
            let selected = unsafe { action.action.search_selected.selected };
            Some(GhosttyRuntimeActionEvent::SearchSelected {
                selected: (selected >= 0).then_some(selected as u64),
            })
        }
        _ => None,
    }
}

unsafe fn runtime_action_c_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    if text.is_empty() { None } else { Some(text) }
}

unsafe fn runtime_action_sized_string(value: *const c_char, len: usize) -> Option<String> {
    if value.is_null() || len == 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), len) };
    let text = String::from_utf8_lossy(bytes).into_owned();
    if text.is_empty() { None } else { Some(text) }
}

/*
CDXC:Clipboard 2026-06-27-04:10:
Ghostty's embedded runtime calls clipboard callbacks with `SurfaceUD`, while wakeup/action still use app-level runtime userdata. Validate the pointer against registered surface tokens before casting, enqueue only standard reads and explicit `text/plain` standard writes, complete initial reads as unconfirmed so Ghostty paste protection can ask back through `confirm_read_clipboard_cb`, and mirror native Ghostex by confirming the borrowed callback content synchronously without storing or logging it.
*/
unsafe extern "C" fn ghostty_runtime_read_clipboard_cb(
    userdata: *mut c_void,
    clipboard: ffi::ghostty_clipboard_e,
    state: *mut c_void,
    mimes: *const *const c_char,
    mimes_len: usize,
    list: bool,
) -> ffi::ghostty_clipboard_read_result_e {
    if clipboard != ffi::GHOSTTY_CLIPBOARD_STANDARD || state.is_null() {
        return ffi::GHOSTTY_CLIPBOARD_READ_UNSUPPORTED;
    }
    if list || !(unsafe { runtime_clipboard_requests_text_plain(mimes, mimes_len) }) {
        return ffi::GHOSTTY_CLIPBOARD_READ_UNAVAILABLE;
    }
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return ffi::GHOSTTY_CLIPBOARD_READ_UNSUPPORTED;
    };
    if unsafe { token.as_ref().enqueue_runtime_clipboard_read(state) } {
        ffi::GHOSTTY_CLIPBOARD_READ_STARTED
    } else {
        ffi::GHOSTTY_CLIPBOARD_READ_UNAVAILABLE
    }
}

unsafe extern "C" fn ghostty_runtime_confirm_read_clipboard_cb(
    userdata: *mut c_void,
    confirm: *const ffi::ghostty_clipboard_confirm_s,
    state: *mut c_void,
    request: ffi::ghostty_clipboard_request_e,
) {
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return;
    };
    if confirm.is_null()
        || state.is_null()
        || !runtime_clipboard_confirm_read_request_supported(request)
    {
        unsafe { token.as_ref().deny_runtime_clipboard_request(state) };
        return;
    }
    unsafe {
        token
            .as_ref()
            .complete_runtime_clipboard_confirmation(confirm, state)
    };
}

unsafe extern "C" fn ghostty_runtime_write_clipboard_cb(
    userdata: *mut c_void,
    clipboard: ffi::ghostty_clipboard_e,
    content: *const ffi::ghostty_clipboard_content_s,
    len: usize,
    _confirm: bool,
) {
    if clipboard != ffi::GHOSTTY_CLIPBOARD_STANDARD {
        return;
    }
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return;
    };
    let Some(text) = (unsafe { runtime_clipboard_text_plain_content(content, len) }) else {
        return;
    };
    unsafe {
        token.as_ref().enqueue_runtime_clipboard_write(text);
    }
}
