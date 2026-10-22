//! The speaker on macOS: `native/macos/GpuiReadAloud.m` (one AVSpeechSynthesizer on the main
//! queue). Its delegate's finish and cancel come back through [`on_ended`] as the chat host's
//! `speechEnded`.

use std::ffi::{CStr, CString, c_char};

mod ffi {
    use std::ffi::c_char;

    pub(super) type Ended = extern "C" fn(u64, i32);

    unsafe extern "C" {
        pub(super) fn GhostexReadAloudSpeak(
            token: u64,
            texts: *const *const c_char,
            count: usize,
            voice: *const c_char,
            rate: f32,
            callback: Ended,
        );
        pub(super) fn GhostexReadAloudStop();
        pub(super) fn GhostexReadAloudVoicesJson() -> *mut c_char;
        pub(super) fn GhostexReadAloudFree(buffer: *mut c_char);
    }
}

/// The main thread hears an utterance end; the chat host is told on its own thread.
extern "C" fn on_ended(token: u64, interrupted: i32) {
    crate::app::gx_chat::read_aloud_speech_ended(token, interrupted != 0);
}

pub(super) fn speak(token: u64, texts: &[String], voice: &str, rate: f32) {
    // A NUL cannot cross as a C string; the engine would read nothing past it anyway.
    let owned: Vec<CString> = texts
        .iter()
        .map(|text| CString::new(text.replace('\0', " ")).unwrap_or_default())
        .collect();
    let pointers: Vec<*const c_char> = owned.iter().map(|text| text.as_ptr()).collect();
    let voice = CString::new(voice.replace('\0', "")).unwrap_or_default();
    // The shim copies every string before it returns.
    unsafe {
        ffi::GhostexReadAloudSpeak(
            token,
            pointers.as_ptr(),
            pointers.len(),
            voice.as_ptr(),
            rate,
            on_ended,
        );
    }
}

pub(super) fn stop() {
    unsafe { ffi::GhostexReadAloudStop() }
}

/// This Mac's voices, as the shim lists them: `{voices: [{id, name, language, languageName,
/// quality, novelty}], currentLanguage}`.
pub(super) fn voices_json() -> Option<serde_json::Value> {
    let raw = unsafe { ffi::GhostexReadAloudVoicesJson() };
    if raw.is_null() {
        return None;
    }
    let parsed = unsafe { CStr::from_ptr(raw) }
        .to_str()
        .ok()
        .and_then(|text| serde_json::from_str(text).ok());
    unsafe { ffi::GhostexReadAloudFree(raw) };
    parsed
}

/// Speed (a multiple of the normal speed) as an `AVSpeechUtterance` rate, between the measured
/// points.
///
/// CDXC:VoiceMode 2026-10-22 WHY: AVSpeech's rate is far from linear around its default of 0.5, so multiplying the default would make 2× unbearably fast and 0.75× barely slower. Measured on this Mac with `AVSpeechSynthesizer.write` (Samantha, one two-sentence reply): rate 0.25 reads at 0.77× the default's speed, 0.54 at 1.24×, 0.585 at 1.51× and 0.66 at 1.96×, so the Speed row's choices map through those points.
pub(super) fn rate_for_speed(speed: f64) -> f32 {
    const POINTS: [(f64, f64); 5] = [
        (0.75, 0.25),
        (1.0, 0.5),
        (1.25, 0.54),
        (1.5, 0.585),
        (2.0, 0.66),
    ];
    let speed = if speed.is_finite() { speed } else { 1.0 };
    let (first, last) = (POINTS[0], POINTS[POINTS.len() - 1]);
    if speed <= first.0 {
        return first.1 as f32;
    }
    if speed >= last.0 {
        return last.1 as f32;
    }
    let rate = POINTS
        .windows(2)
        .find(|pair| speed <= pair[1].0)
        .map(|pair| {
            let ((low_speed, low_rate), (high_speed, high_rate)) = (pair[0], pair[1]);
            low_rate + (high_rate - low_rate) * (speed - low_speed) / (high_speed - low_speed)
        })
        .unwrap_or(0.5);
    rate as f32
}
