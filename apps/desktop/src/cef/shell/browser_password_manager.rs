//! Turns off Chromium's built-in password manager in Browser profiles.

use cef::{CefString, ImplPreferenceManager as _, ImplValue as _, RequestContext};

/// CDXC:Browser 2026-10-17 DECISION:
/// User, on Chromium's built-in password manager in the Browser: "Turn it off". It only half-works in Ghostex tabs: Ghostex's embedded (Alloy-style) tabs never show Chrome's save-password prompt, and CEF runs with `--use-mock-keychain`, so anything it stored would not be protected by the real macOS Keychain. Saving and automatic sign-in are turned off in every disk-backed profile (the Default profile, which shares CEF's global context, and each workspace profile); memory-only profiles are off-the-record, where Chromium never saves and reads these prefs from the Default profile. Ordinary form autofill (addresses, other saved values) stays on, and nothing already saved is deleted. Chromium has no pref that stops it from offering passwords that are already saved; these profiles hold none because no save prompt ever appeared.
pub(crate) fn turn_off_browser_password_manager(context: &RequestContext) {
    for name in [
        "credentials_enable_service",
        "credentials_enable_autosignin",
    ] {
        if let Err(error) = set_boolean_preference(context, name, false) {
            eprintln!("[gpui-cef] could not turn off the Browser password manager: {error}");
        }
    }
}

/// Uses the same writable error slot as the remote proxy (CDXC:Browser 2026-09-18 in remote_browser.rs).
fn set_boolean_preference(context: &RequestContext, name: &str, value: bool) -> Result<(), String> {
    let name_string = CefString::from(name);
    if context.has_preference(Some(&name_string)) == 0 {
        return Err(format!("{name} is not a registered preference"));
    }
    let mut pref_value = cef::value_create().ok_or_else(|| format!("create {name} value"))?;
    pref_value.set_bool(value as i32);
    let mut error_buf: cef::sys::_cef_string_utf16_t = unsafe { std::mem::zeroed() };
    let mut error = CefString::from(&mut error_buf as *mut cef::sys::_cef_string_utf16_t);
    let set =
        context.set_preference(Some(&name_string), Some(&mut pref_value), Some(&mut error)) != 0;
    let failure = error.to_string();
    unsafe { cef::sys::cef_string_utf16_clear(&mut error_buf) };
    if set {
        Ok(())
    } else {
        Err(format!("{name}: {failure}"))
    }
}
