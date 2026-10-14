use std::ffi::{c_char, c_int, c_void};

use crate::ghostty_kit::ffi;

#[derive(Clone, Copy)]
pub(crate) struct GhosttyKitFunctionTable {
    pub(crate) init: unsafe fn(usize, *mut *mut c_char) -> c_int,
    pub(crate) config_new: unsafe fn() -> ffi::ghostty_config_t,
    pub(crate) config_free: unsafe fn(ffi::ghostty_config_t),
    pub(crate) config_load_file: unsafe fn(ffi::ghostty_config_t, *const c_char),
    pub(crate) config_load_string: unsafe fn(ffi::ghostty_config_t, *const c_char, usize),
    pub(crate) config_load_recursive_files: unsafe fn(ffi::ghostty_config_t),
    pub(crate) config_finalize: unsafe fn(ffi::ghostty_config_t),
    pub(crate) app_new: unsafe fn(
        *const ffi::ghostty_runtime_config_s,
        ffi::ghostty_config_t,
    ) -> ffi::ghostty_app_t,
    pub(crate) app_free: unsafe fn(ffi::ghostty_app_t),
    pub(crate) app_tick: unsafe fn(ffi::ghostty_app_t),
    pub(crate) app_set_focus: unsafe fn(ffi::ghostty_app_t, bool),
    pub(crate) string_free: unsafe fn(ffi::ghostty_string_s),
    pub(crate) surface_config_new: unsafe fn() -> ffi::ghostty_surface_config_s,
    pub(crate) surface_new: unsafe fn(
        ffi::ghostty_app_t,
        *const ffi::ghostty_surface_config_s,
    ) -> ffi::ghostty_surface_t,
    pub(crate) surface_free: unsafe fn(ffi::ghostty_surface_t),
    pub(crate) surface_set_content_scale: unsafe fn(ffi::ghostty_surface_t, f64, f64),
    pub(crate) surface_set_size: unsafe fn(ffi::ghostty_surface_t, u32, u32),
    pub(crate) surface_set_focus: unsafe fn(ffi::ghostty_surface_t, bool),
    pub(crate) surface_set_occlusion: unsafe fn(ffi::ghostty_surface_t, bool),
    pub(crate) surface_size: unsafe fn(ffi::ghostty_surface_t) -> ffi::ghostty_surface_size_s,
    pub(crate) surface_needs_confirm_quit: unsafe fn(ffi::ghostty_surface_t) -> bool,
    pub(crate) surface_binding_action:
        unsafe fn(ffi::ghostty_surface_t, *const c_char, usize) -> bool,
    pub(crate) surface_process_exited: unsafe fn(ffi::ghostty_surface_t) -> bool,
    #[allow(dead_code)]
    // ghostty surface FFI vtable entry kept complete; nothing reads this metadata back today
    pub(crate) surface_foreground_pid: unsafe fn(ffi::ghostty_surface_t) -> u64,
    #[allow(dead_code)]
    // ghostty surface FFI vtable entry kept complete; nothing reads this metadata back today
    pub(crate) surface_tty_name: unsafe fn(ffi::ghostty_surface_t) -> ffi::ghostty_string_s,
    pub(crate) surface_key_translation_mods:
        unsafe fn(ffi::ghostty_surface_t, ffi::ghostty_input_mods_e) -> ffi::ghostty_input_mods_e,
    pub(crate) surface_key: unsafe fn(ffi::ghostty_surface_t, ffi::ghostty_input_key_s) -> bool,
    pub(crate) surface_key_is_binding: unsafe fn(
        ffi::ghostty_surface_t,
        ffi::ghostty_input_key_s,
        *mut ffi::ghostty_binding_flags_e,
    ) -> bool,
    pub(crate) surface_text: unsafe fn(ffi::ghostty_surface_t, *const c_char, usize),
    pub(crate) surface_preedit: unsafe fn(ffi::ghostty_surface_t, *const c_char, usize),
    pub(crate) surface_mouse_captured: unsafe fn(ffi::ghostty_surface_t) -> bool,
    pub(crate) surface_mouse_button: unsafe fn(
        ffi::ghostty_surface_t,
        ffi::ghostty_input_mouse_state_e,
        ffi::ghostty_input_mouse_button_e,
        ffi::ghostty_input_mods_e,
    ) -> bool,
    pub(crate) surface_mouse_pos:
        unsafe fn(ffi::ghostty_surface_t, f64, f64, ffi::ghostty_input_mods_e),
    pub(crate) surface_mouse_scroll:
        unsafe fn(ffi::ghostty_surface_t, f64, f64, ffi::ghostty_input_scroll_mods_t),
    pub(crate) surface_mouse_pressure: unsafe fn(ffi::ghostty_surface_t, u32, f64),
    pub(crate) surface_ime_point:
        unsafe fn(ffi::ghostty_surface_t, *mut f64, *mut f64, *mut f64, *mut f64),
    pub(crate) surface_request_close: unsafe fn(ffi::ghostty_surface_t),
    pub(crate) surface_complete_clipboard_request:
        unsafe fn(ffi::ghostty_surface_t, *const ffi::ghostty_clipboard_complete_s, *mut c_void),
    pub(crate) surface_deny_clipboard_request: unsafe fn(ffi::ghostty_surface_t, *mut c_void),
}

impl GhosttyKitFunctionTable {
    /*
    CDXC:PlatformSupport 2026-07-05:
    The production table binds the real GhosttyKit exports, which exist only
    in the macOS static archive (gpui/build.rs). Non-macOS terminals run the
    libghostty-vt GPUI engine instead, so the table constructor and every
    production_* binding below are macOS-only; the table type itself stays
    cross-platform because owner structs carry it by value.
    */
    #[cfg(target_os = "macos")]
    pub(crate) const fn production() -> Self {
        Self {
            init: production_ghostty_init,
            config_new: production_ghostty_config_new,
            config_free: production_ghostty_config_free,
            config_load_file: production_ghostty_config_load_file,
            config_load_string: production_ghostty_config_load_string,
            config_load_recursive_files: production_ghostty_config_load_recursive_files,
            config_finalize: production_ghostty_config_finalize,
            app_new: production_ghostty_app_new,
            app_free: production_ghostty_app_free,
            app_tick: production_ghostty_app_tick,
            app_set_focus: production_ghostty_app_set_focus,
            string_free: production_ghostty_string_free,
            surface_config_new: production_ghostty_surface_config_new,
            surface_new: production_ghostty_surface_new,
            surface_free: production_ghostty_surface_free,
            surface_set_content_scale: production_ghostty_surface_set_content_scale,
            surface_set_size: production_ghostty_surface_set_size,
            surface_set_focus: production_ghostty_surface_set_focus,
            surface_set_occlusion: production_ghostty_surface_set_occlusion,
            surface_size: production_ghostty_surface_size,
            surface_needs_confirm_quit: production_ghostty_surface_needs_confirm_quit,
            surface_binding_action: production_ghostty_surface_binding_action,
            surface_process_exited: production_ghostty_surface_process_exited,
            surface_foreground_pid: production_ghostty_surface_foreground_pid,
            surface_tty_name: production_ghostty_surface_tty_name,
            surface_key_translation_mods: production_ghostty_surface_key_translation_mods,
            surface_key: production_ghostty_surface_key,
            surface_key_is_binding: production_ghostty_surface_key_is_binding,
            surface_text: production_ghostty_surface_text,
            surface_preedit: production_ghostty_surface_preedit,
            surface_mouse_captured: production_ghostty_surface_mouse_captured,
            surface_mouse_button: production_ghostty_surface_mouse_button,
            surface_mouse_pos: production_ghostty_surface_mouse_pos,
            surface_mouse_scroll: production_ghostty_surface_mouse_scroll,
            surface_mouse_pressure: production_ghostty_surface_mouse_pressure,
            surface_ime_point: production_ghostty_surface_ime_point,
            surface_request_close: production_ghostty_surface_request_close,
            surface_complete_clipboard_request:
                production_ghostty_surface_complete_clipboard_request,
            surface_deny_clipboard_request: production_ghostty_surface_deny_clipboard_request,
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_init(argc: usize, argv: *mut *mut c_char) -> c_int {
    unsafe { ffi::ghostty_init(argc, argv) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_new() -> ffi::ghostty_config_t {
    unsafe { ffi::ghostty_config_new() }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_free(config: ffi::ghostty_config_t) {
    unsafe { ffi::ghostty_config_free(config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_load_file(config: ffi::ghostty_config_t, path: *const c_char) {
    unsafe { ffi::ghostty_config_load_file(config, path) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_load_string(
    config: ffi::ghostty_config_t,
    source: *const c_char,
    len: usize,
) {
    unsafe { ffi::ghostty_config_load_string(config, source, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_load_recursive_files(config: ffi::ghostty_config_t) {
    unsafe { ffi::ghostty_config_load_recursive_files(config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_finalize(config: ffi::ghostty_config_t) {
    unsafe { ffi::ghostty_config_finalize(config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_new(
    runtime_config: *const ffi::ghostty_runtime_config_s,
    config: ffi::ghostty_config_t,
) -> ffi::ghostty_app_t {
    unsafe { ffi::ghostty_app_new(runtime_config, config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_free(app: ffi::ghostty_app_t) {
    unsafe { ffi::ghostty_app_free(app) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_tick(app: ffi::ghostty_app_t) {
    unsafe { ffi::ghostty_app_tick(app) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_set_focus(app: ffi::ghostty_app_t, focused: bool) {
    unsafe { ffi::ghostty_app_set_focus(app, focused) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_string_free(value: ffi::ghostty_string_s) {
    unsafe { ffi::ghostty_string_free(value) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_config_new() -> ffi::ghostty_surface_config_s {
    unsafe { ffi::ghostty_surface_config_new() }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_new(
    app: ffi::ghostty_app_t,
    config: *const ffi::ghostty_surface_config_s,
) -> ffi::ghostty_surface_t {
    unsafe { ffi::ghostty_surface_new(app, config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_free(surface: ffi::ghostty_surface_t) {
    unsafe { ffi::ghostty_surface_free(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_content_scale(
    surface: ffi::ghostty_surface_t,
    x: f64,
    y: f64,
) {
    unsafe { ffi::ghostty_surface_set_content_scale(surface, x, y) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_size(
    surface: ffi::ghostty_surface_t,
    width: u32,
    height: u32,
) {
    unsafe { ffi::ghostty_surface_set_size(surface, width, height) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_focus(surface: ffi::ghostty_surface_t, focused: bool) {
    unsafe { ffi::ghostty_surface_set_focus(surface, focused) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_occlusion(surface: ffi::ghostty_surface_t, visible: bool) {
    unsafe { ffi::ghostty_surface_set_occlusion(surface, visible) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_size(
    surface: ffi::ghostty_surface_t,
) -> ffi::ghostty_surface_size_s {
    unsafe { ffi::ghostty_surface_size(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_process_exited(surface: ffi::ghostty_surface_t) -> bool {
    unsafe { ffi::ghostty_surface_process_exited(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_needs_confirm_quit(surface: ffi::ghostty_surface_t) -> bool {
    unsafe { ffi::ghostty_surface_needs_confirm_quit(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_binding_action(
    surface: ffi::ghostty_surface_t,
    action: *const c_char,
    len: usize,
) -> bool {
    unsafe { ffi::ghostty_surface_binding_action(surface, action, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_foreground_pid(surface: ffi::ghostty_surface_t) -> u64 {
    unsafe { ffi::ghostty_surface_foreground_pid(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_tty_name(
    surface: ffi::ghostty_surface_t,
) -> ffi::ghostty_string_s {
    unsafe { ffi::ghostty_surface_tty_name(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_key_translation_mods(
    surface: ffi::ghostty_surface_t,
    mods: ffi::ghostty_input_mods_e,
) -> ffi::ghostty_input_mods_e {
    unsafe { ffi::ghostty_surface_key_translation_mods(surface, mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_key(
    surface: ffi::ghostty_surface_t,
    event: ffi::ghostty_input_key_s,
) -> bool {
    unsafe { ffi::ghostty_surface_key(surface, event) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_key_is_binding(
    surface: ffi::ghostty_surface_t,
    event: ffi::ghostty_input_key_s,
    flags: *mut ffi::ghostty_binding_flags_e,
) -> bool {
    unsafe { ffi::ghostty_surface_key_is_binding(surface, event, flags) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_text(
    surface: ffi::ghostty_surface_t,
    ptr: *const c_char,
    len: usize,
) {
    unsafe { ffi::ghostty_surface_text(surface, ptr, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_preedit(
    surface: ffi::ghostty_surface_t,
    ptr: *const c_char,
    len: usize,
) {
    unsafe { ffi::ghostty_surface_preedit(surface, ptr, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_captured(surface: ffi::ghostty_surface_t) -> bool {
    unsafe { ffi::ghostty_surface_mouse_captured(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_button(
    surface: ffi::ghostty_surface_t,
    action: ffi::ghostty_input_mouse_state_e,
    button: ffi::ghostty_input_mouse_button_e,
    mods: ffi::ghostty_input_mods_e,
) -> bool {
    unsafe { ffi::ghostty_surface_mouse_button(surface, action, button, mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_pos(
    surface: ffi::ghostty_surface_t,
    x: f64,
    y: f64,
    mods: ffi::ghostty_input_mods_e,
) {
    unsafe { ffi::ghostty_surface_mouse_pos(surface, x, y, mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_scroll(
    surface: ffi::ghostty_surface_t,
    x: f64,
    y: f64,
    scroll_mods: ffi::ghostty_input_scroll_mods_t,
) {
    unsafe { ffi::ghostty_surface_mouse_scroll(surface, x, y, scroll_mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_pressure(
    surface: ffi::ghostty_surface_t,
    stage: u32,
    pressure: f64,
) {
    unsafe { ffi::ghostty_surface_mouse_pressure(surface, stage, pressure) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_ime_point(
    surface: ffi::ghostty_surface_t,
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) {
    unsafe { ffi::ghostty_surface_ime_point(surface, x, y, width, height) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_request_close(surface: ffi::ghostty_surface_t) {
    unsafe { ffi::ghostty_surface_request_close(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_complete_clipboard_request(
    surface: ffi::ghostty_surface_t,
    complete: *const ffi::ghostty_clipboard_complete_s,
    state: *mut c_void,
) {
    unsafe { ffi::ghostty_surface_complete_clipboard_request(surface, complete, state) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_deny_clipboard_request(
    surface: ffi::ghostty_surface_t,
    state: *mut c_void,
) {
    unsafe { ffi::ghostty_surface_deny_clipboard_request(surface, state) }
}
