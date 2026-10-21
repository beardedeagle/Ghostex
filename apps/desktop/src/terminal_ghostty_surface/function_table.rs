use std::ffi::{c_char, c_int};

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
    pub(crate) string_free: unsafe fn(ffi::ghostty_string_s),
}

impl GhosttyKitFunctionTable {
    /*
    CDXC:PlatformSupport 2026-07-05:
    The production table binds the real GhosttyKit config exports, which exist
    only in the macOS static archive (apps/desktop/build.rs). The table
    constructor and every production_* binding below are macOS-only; the
    table type itself stays cross-platform for the shared config loader.
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
            string_free: production_ghostty_string_free,
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
unsafe fn production_ghostty_string_free(value: ffi::ghostty_string_s) {
    unsafe { ffi::ghostty_string_free(value) }
}
