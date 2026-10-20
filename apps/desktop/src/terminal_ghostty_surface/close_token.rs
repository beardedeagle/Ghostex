use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::{CStr, c_char, c_void},
    mem::{self},
    ptr::{self, NonNull},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicPtr, AtomicU8, Ordering},
    },
};

use crate::ghostty_kit::ffi;

use super::*;

const GHOSTTY_SURFACE_CLOSE_STATE_NONE: u8 = 0;
const GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED: u8 = 1;
const GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED: u8 = 2;

/*
CDXC:Clipboard 2026-06-27-04:10:
Owner-local clipboard operations are the only bridge from Ghostty callbacks to GPUI clipboard APIs. A denied drain completes pending reads with empty data and drops writes without invoking clipboard closures; an allowed drain may read/write only for the exact mounted owner that holds this token, never through focus, app-level userdata, logs, persistence, or selection clipboard routing.
*/
enum GhosttyRuntimeClipboardOperation {
    ReadStandard { state: *mut c_void },
    WriteStandardText { text: String },
}

/// Bound on undrained per-surface runtime action events. The app thread drains
/// every frame while a surface is mounted, so the cap only matters for surfaces
/// that emit actions while hidden; oldest events drop first.
const GHOSTTY_RUNTIME_ACTION_EVENT_QUEUE_LIMIT: usize = 128;

/// Ghostty runtime actions surfaced to the app thread. Strings are copied out
/// of the callback-scoped C pointers before crossing threads.
#[derive(Clone, Debug)]
pub(crate) enum GhosttyRuntimeActionEvent {
    OpenUrl { url: String },
    RingBell,
    SetTitle { title: String },
    Pwd { pwd: String },
    MouseOverLink { url: Option<String> },
    StartSearch { needle: Option<String> },
    EndSearch,
    SearchTotal { total: Option<u64> },
    SearchSelected { selected: Option<u64> },
}

pub(crate) struct GhosttySurfaceCloseToken {
    close_state: AtomicU8,
    surface: AtomicPtr<c_void>,
    surface_complete_clipboard_request:
        unsafe fn(ffi::ghostty_surface_t, *const ffi::ghostty_clipboard_complete_s, *mut c_void),
    surface_deny_clipboard_request: unsafe fn(ffi::ghostty_surface_t, *mut c_void),
    runtime_clipboard_operations: Mutex<VecDeque<GhosttyRuntimeClipboardOperation>>,
    runtime_action_events: Mutex<VecDeque<GhosttyRuntimeActionEvent>>,
}

impl GhosttySurfaceCloseToken {
    pub(crate) fn boxed(functions: GhosttyKitFunctionTable) -> Box<Self> {
        let token = Box::new(Self::new(functions));
        token.register_surface_userdata();
        token
    }

    fn new(functions: GhosttyKitFunctionTable) -> Self {
        Self {
            close_state: AtomicU8::new(GHOSTTY_SURFACE_CLOSE_STATE_NONE),
            surface: AtomicPtr::new(ptr::null_mut()),
            surface_complete_clipboard_request: functions.surface_complete_clipboard_request,
            surface_deny_clipboard_request: functions.surface_deny_clipboard_request,
            runtime_clipboard_operations: Mutex::new(VecDeque::new()),
            runtime_action_events: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) fn as_userdata(&self) -> *mut c_void {
        self as *const GhosttySurfaceCloseToken as *mut c_void
    }

    fn userdata_key(&self) -> usize {
        self.as_userdata() as usize
    }

    fn register_surface_userdata(&self) {
        if let Ok(mut tokens) = ghostty_surface_close_token_registry().lock() {
            tokens.insert(self.userdata_key());
        }
    }

    fn unregister_surface_userdata(&self) {
        if let Ok(mut tokens) = ghostty_surface_close_token_registry().lock() {
            tokens.remove(&self.userdata_key());
        }
    }

    pub(crate) fn set_surface(&self, surface: ffi::ghostty_surface_t) {
        self.surface.store(surface, Ordering::SeqCst);
        // Runtime action callbacks identify surfaces by pointer (not userdata),
        // so keep a surface-pointer index alongside the userdata registry.
        if !surface.is_null()
            && let Ok(mut surfaces) = ghostty_surface_action_token_registry().lock()
        {
            surfaces.insert(surface as usize, self.userdata_key());
        }
    }

    pub(crate) fn clear_surface(&self) {
        let previous = self.surface.swap(ptr::null_mut(), Ordering::SeqCst);
        if previous.is_null() {
            return;
        }
        if let Ok(mut surfaces) = ghostty_surface_action_token_registry().lock() {
            surfaces.remove(&(previous as usize));
        }
    }

    fn runtime_surface(&self) -> Option<ffi::ghostty_surface_t> {
        NonNull::new(self.surface.load(Ordering::SeqCst)).map(NonNull::as_ptr)
    }

    fn record_close_callback(&self, confirmation_needed: bool) {
        let state = if confirmation_needed {
            GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED
        } else {
            GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED
        };
        self.close_state.store(state, Ordering::SeqCst);
    }

    pub(crate) fn enqueue_runtime_action_event(&self, event: GhosttyRuntimeActionEvent) {
        if self.runtime_surface().is_none() {
            return;
        }
        if let Ok(mut events) = self.runtime_action_events.lock() {
            if events.len() >= GHOSTTY_RUNTIME_ACTION_EVENT_QUEUE_LIMIT {
                events.pop_front();
            }
            events.push_back(event);
        }
    }

    pub(crate) fn take_runtime_action_events(&self) -> VecDeque<GhosttyRuntimeActionEvent> {
        self.runtime_action_events
            .lock()
            .map(|mut events| mem::take(&mut *events))
            .unwrap_or_default()
    }

    pub(crate) fn enqueue_runtime_clipboard_read(&self, state: *mut c_void) -> bool {
        if self.runtime_surface().is_none() {
            return false;
        }
        let Ok(mut operations) = self.runtime_clipboard_operations.lock() else {
            return false;
        };
        operations.push_back(GhosttyRuntimeClipboardOperation::ReadStandard { state });
        true
    }

    pub(crate) fn enqueue_runtime_clipboard_write(&self, text: String) {
        if self.runtime_surface().is_none() || text.is_empty() {
            return;
        }
        if let Ok(mut operations) = self.runtime_clipboard_operations.lock() {
            operations.push_back(GhosttyRuntimeClipboardOperation::WriteStandardText { text });
        }
    }

    pub(crate) fn drain_runtime_clipboard_operations(
        &self,
        allow_standard_clipboard: bool,
        mut read_standard_text: impl FnMut() -> Option<String>,
        mut write_standard_text: impl FnMut(String),
    ) {
        let operations = self.take_runtime_clipboard_operations();
        for operation in operations {
            match operation {
                GhosttyRuntimeClipboardOperation::ReadStandard { state } => {
                    let text = if allow_standard_clipboard {
                        read_standard_text()
                    } else {
                        None
                    };
                    self.complete_runtime_clipboard_read(state, text);
                }
                GhosttyRuntimeClipboardOperation::WriteStandardText { text } => {
                    if allow_standard_clipboard {
                        write_standard_text(text);
                    }
                }
            }
        }
    }

    pub(crate) fn deny_pending_runtime_clipboard_operations(&self) {
        let operations = self.take_runtime_clipboard_operations();
        for operation in operations {
            if let GhosttyRuntimeClipboardOperation::ReadStandard { state } = operation {
                self.deny_runtime_clipboard_request(state);
            }
        }
    }

    fn take_runtime_clipboard_operations(&self) -> VecDeque<GhosttyRuntimeClipboardOperation> {
        self.runtime_clipboard_operations
            .lock()
            .map(|mut operations| mem::take(&mut *operations))
            .unwrap_or_default()
    }

    fn complete_runtime_clipboard_read(&self, state: *mut c_void, text: Option<String>) {
        let text = text.map(String::into_bytes);
        let content = text.as_ref().map(|bytes| ffi::ghostty_clipboard_content_s {
            mime: GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_C_STRING
                .as_ptr()
                .cast(),
            data: bytes.as_ptr().cast(),
            len: bytes.len(),
        });
        let complete = ffi::ghostty_clipboard_complete_s {
            contents: content
                .as_ref()
                .map_or(ptr::null(), |content| content as *const _),
            contents_len: usize::from(content.is_some()),
            available: ptr::null(),
            available_len: 0,
            confirmed: false,
            remember: false,
        };
        self.complete_runtime_clipboard_request(&complete, state);
    }

    fn complete_runtime_clipboard_request(
        &self,
        complete: *const ffi::ghostty_clipboard_complete_s,
        state: *mut c_void,
    ) {
        let Some(surface) = self.runtime_surface() else {
            return;
        };
        unsafe {
            (self.surface_complete_clipboard_request)(surface, complete, state);
        }
    }

    pub(crate) unsafe fn complete_runtime_clipboard_confirmation(
        &self,
        confirm: *const ffi::ghostty_clipboard_confirm_s,
        state: *mut c_void,
    ) {
        let Some(confirm) = (unsafe { confirm.as_ref() }) else {
            self.deny_runtime_clipboard_request(state);
            return;
        };
        let complete = ffi::ghostty_clipboard_complete_s {
            contents: confirm.contents,
            contents_len: confirm.contents_len,
            available: confirm.available,
            available_len: confirm.available_len,
            confirmed: true,
            remember: false,
        };
        self.complete_runtime_clipboard_request(&complete, state);
    }

    pub(crate) fn deny_runtime_clipboard_request(&self, state: *mut c_void) {
        let Some(surface) = self.runtime_surface() else {
            return;
        };
        unsafe { (self.surface_deny_clipboard_request)(surface, state) };
    }

    pub(crate) fn consume_confirmed_close_requested(&self) -> bool {
        self.close_state
            .compare_exchange(
                GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED,
                GHOSTTY_SURFACE_CLOSE_STATE_NONE,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }

    pub(crate) fn consume_confirmation_needed_close_requested(&self) -> bool {
        self.close_state
            .compare_exchange(
                GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED,
                GHOSTTY_SURFACE_CLOSE_STATE_NONE,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }

    pub(crate) fn confirmed_close_pending(&self) -> bool {
        self.close_state.load(Ordering::SeqCst) == GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED
    }

    pub(crate) fn clear_confirmation_needed_close_requested(&self) {
        let _ = self.close_state.compare_exchange(
            GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED,
            GHOSTTY_SURFACE_CLOSE_STATE_NONE,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
}

impl Drop for GhosttySurfaceCloseToken {
    fn drop(&mut self) {
        self.clear_surface();
        self.unregister_surface_userdata();
    }
}

fn ghostty_surface_action_token_registry() -> &'static Mutex<HashMap<usize, usize>> {
    static SURFACES: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    SURFACES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn registered_surface_close_token_for_action_target(
    target: ffi::ghostty_target_s,
) -> Option<NonNull<GhosttySurfaceCloseToken>> {
    if target.tag != ffi::GHOSTTY_TARGET_SURFACE {
        return None;
    }
    let surface = unsafe { target.target.surface };
    if surface.is_null() {
        return None;
    }
    let userdata = ghostty_surface_action_token_registry()
        .lock()
        .ok()?
        .get(&(surface as usize))
        .copied()?;
    registered_surface_close_token_from_userdata(userdata as *mut c_void)
}

fn ghostty_surface_close_token_registry() -> &'static Mutex<HashSet<usize>> {
    static TOKENS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    TOKENS.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn registered_surface_close_token_from_userdata(
    userdata: *mut c_void,
) -> Option<NonNull<GhosttySurfaceCloseToken>> {
    let token = NonNull::new(userdata as *mut GhosttySurfaceCloseToken)?;
    let is_registered = ghostty_surface_close_token_registry()
        .lock()
        .map(|tokens| tokens.contains(&(userdata as usize)))
        .unwrap_or(false);
    if is_registered { Some(token) } else { None }
}

pub(crate) fn runtime_clipboard_confirm_read_request_supported(
    request: ffi::ghostty_clipboard_request_e,
) -> bool {
    request == ffi::GHOSTTY_CLIPBOARD_REQUEST_PASTE
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_KITTY_READ
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_KITTY_WRITE
}

pub(crate) unsafe fn runtime_clipboard_requests_text_plain(
    mimes: *const *const c_char,
    len: usize,
) -> bool {
    if mimes.is_null() || len == 0 {
        return false;
    }
    unsafe { std::slice::from_raw_parts(mimes, len) }
        .iter()
        .copied()
        .filter(|mime| !mime.is_null())
        .any(|mime| {
            unsafe { CStr::from_ptr(mime) }.to_bytes() == GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_MIME
        })
}

pub(crate) unsafe fn runtime_clipboard_text_plain_content(
    content: *const ffi::ghostty_clipboard_content_s,
    len: usize,
) -> Option<String> {
    if content.is_null() || len == 0 {
        return None;
    }
    for entry in unsafe { std::slice::from_raw_parts(content, len) } {
        if entry.mime.is_null() || entry.data.is_null() {
            continue;
        }
        let mime = unsafe { CStr::from_ptr(entry.mime) };
        if mime.to_bytes() != GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_MIME {
            continue;
        }
        let data = unsafe { std::slice::from_raw_parts(entry.data.cast::<u8>(), entry.len) };
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        return Some(text.to_string());
    }
    None
}

pub(crate) unsafe extern "C" fn ghostty_runtime_close_surface_cb(
    userdata: *mut c_void,
    confirmation_needed: bool,
) {
    /*
    CDXC:Terminal 2026-06-27-04:25:
    Ghostty close callbacks use the same surface userdata channel as clipboard callbacks. Validate the pointer against registered surface tokens before mutating owner-local close state so app-level runtime userdata and stale pointers cannot be treated as terminal owners.
    */
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return;
    };
    unsafe {
        token.as_ref().record_close_callback(confirmation_needed);
    }
}
