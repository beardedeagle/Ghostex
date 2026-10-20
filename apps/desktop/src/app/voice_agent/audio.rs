//! The microphone and speaker of a voice conversation and of Grok dictation: 24 kHz mono PCM16
//! both ways, the format gxserver relays to the provider unchanged.
//!
//! On macOS this is `native/macos/GpuiVoiceAudio.m` (AVAudioEngine; a conversation turns on the
//! voice-processing I/O unit, whose echo canceller keeps the agent's voice from the speakers out of
//! the microphone). Other systems have no implementation yet and report `Unsupported`.

use std::sync::Mutex;

use super::test_mic::TestMic;

/// Receives captured microphone audio on the audio thread. It must only queue it and return.
pub(crate) type CaptureFn = Box<dyn Fn(&[i16]) + Send + Sync>;
/// Called on an audio thread when the input or output device changed and the engine stopped; the
/// owner starts the audio again.
pub(crate) type DeviceChangedFn = Box<dyn Fn() + Send + Sync>;

/// Why the audio did not start.
#[derive(Clone, Debug)]
pub(crate) enum AudioStartError {
    PermissionDenied,
    NoInput,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Unsupported,
    Failed(String),
}

impl AudioStartError {
    /// A sentence for the user, naming the setting to change when there is one.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::PermissionDenied => "Ghostex can't use the microphone. Allow Ghostex under Microphone in System Settings > Privacy & Security.".to_string(),
            Self::NoInput => "No microphone was found. Connect one, or choose an input device in your sound settings.".to_string(),
            Self::Unsupported => "Voice is not available on this computer yet.".to_string(),
            Self::Failed(message) => message.clone(),
        }
    }
}

/// What the audio thread calls back into. Lives at a fixed address for as long as the engine runs.
struct CaptureContext {
    capture: Mutex<Option<CaptureFn>>,
    device_changed: Mutex<Option<DeviceChangedFn>>,
    test_mic: Option<Mutex<TestMic>>,
}

/// A running microphone (and, for a conversation, speaker). Dropping it stops both.
pub(crate) struct VoiceAudio {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    handle: usize,
    context: *const CaptureContext,
}

// The native calls synchronize on the engine object; the context is only read through its mutexes.
unsafe impl Send for VoiceAudio {}
unsafe impl Sync for VoiceAudio {}

#[cfg(target_os = "macos")]
mod ffi {
    use std::ffi::{c_char, c_void};

    pub(super) type Capture = extern "C" fn(*mut c_void, *const i16, usize);
    pub(super) type Event = extern "C" fn(*mut c_void, i32);

    unsafe extern "C" {
        pub(super) fn GhostexVoiceAudioStart(
            voice_processing: i32,
            playback: i32,
            capture: Capture,
            event: Event,
            context: *mut c_void,
            out: *mut *mut c_void,
            error_buffer: *mut c_char,
            error_length: usize,
        ) -> i32;
        pub(super) fn GhostexVoiceAudioEnqueue(
            handle: *mut c_void,
            samples: *const i16,
            count: usize,
        );
        pub(super) fn GhostexVoiceAudioFlush(handle: *mut c_void) -> i64;
        pub(super) fn GhostexVoiceAudioStop(handle: *mut c_void);
    }
}

#[cfg(target_os = "macos")]
extern "C" fn on_capture(context: *mut std::ffi::c_void, samples: *const i16, count: usize) {
    if context.is_null() || samples.is_null() || count == 0 {
        return;
    }
    let context = unsafe { &*(context as *const CaptureContext) };
    let samples = unsafe { std::slice::from_raw_parts(samples, count) };
    let Ok(capture) = context.capture.lock() else {
        return;
    };
    let Some(capture) = capture.as_ref() else {
        return;
    };
    match &context.test_mic {
        Some(test_mic) => {
            let mut replaced = vec![0i16; count];
            if let Ok(mut test_mic) = test_mic.lock() {
                test_mic.fill(&mut replaced);
            }
            capture(&replaced);
        }
        None => capture(samples),
    }
}

#[cfg(target_os = "macos")]
extern "C" fn on_event(context: *mut std::ffi::c_void, _event: i32) {
    if context.is_null() {
        return;
    }
    let context = unsafe { &*(context as *const CaptureContext) };
    if let Ok(device_changed) = context.device_changed.lock()
        && let Some(device_changed) = device_changed.as_ref()
    {
        device_changed();
    }
}

impl VoiceAudio {
    /// Opens the microphone (with echo cancellation when `voice_processing`) and, when `playback`,
    /// the speaker. Blocks while macOS asks for microphone access the first time, so call it off
    /// the main thread.
    pub(crate) fn start(
        voice_processing: bool,
        playback: bool,
        capture: CaptureFn,
        device_changed: DeviceChangedFn,
    ) -> Result<Self, AudioStartError> {
        let context = Box::into_raw(Box::new(CaptureContext {
            capture: Mutex::new(Some(capture)),
            device_changed: Mutex::new(Some(device_changed)),
            test_mic: TestMic::from_env().map(Mutex::new),
        }));
        match Self::start_native(voice_processing, playback, context) {
            Ok(handle) => Ok(Self { handle, context }),
            Err(error) => {
                drop(unsafe { Box::from_raw(context) });
                Err(error)
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn start_native(
        voice_processing: bool,
        playback: bool,
        context: *mut CaptureContext,
    ) -> Result<usize, AudioStartError> {
        let mut handle = std::ptr::null_mut();
        let mut error = [0 as std::ffi::c_char; 512];
        let status = unsafe {
            ffi::GhostexVoiceAudioStart(
                voice_processing as i32,
                playback as i32,
                on_capture,
                on_event,
                context as *mut std::ffi::c_void,
                &mut handle,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        match status {
            0 if !handle.is_null() => Ok(handle as usize),
            1 => Err(AudioStartError::PermissionDenied),
            2 => Err(AudioStartError::NoInput),
            _ => {
                let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }
                    .to_string_lossy()
                    .trim()
                    .to_string();
                Err(AudioStartError::Failed(if message.is_empty() {
                    "The microphone could not start.".to_string()
                } else {
                    message
                }))
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn start_native(
        _voice_processing: bool,
        _playback: bool,
        _context: *mut CaptureContext,
    ) -> Result<usize, AudioStartError> {
        Err(AudioStartError::Unsupported)
    }

    /// Queues agent audio (PCM16 24 kHz mono, little-endian bytes) after what is playing.
    pub(crate) fn enqueue(&self, pcm: &[u8]) {
        #[cfg(target_os = "macos")]
        {
            let samples: Vec<i16> = pcm
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            unsafe {
                ffi::GhostexVoiceAudioEnqueue(
                    self.handle as *mut std::ffi::c_void,
                    samples.as_ptr(),
                    samples.len(),
                )
            };
        }
        #[cfg(not(target_os = "macos"))]
        let _ = pcm;
    }

    /// Stops the agent's audio at once and drops what was queued. Answers how many samples were
    /// dropped.
    pub(crate) fn flush(&self) -> u64 {
        #[cfg(target_os = "macos")]
        return unsafe { ffi::GhostexVoiceAudioFlush(self.handle as *mut std::ffi::c_void) }.max(0)
            as u64;
        #[cfg(not(target_os = "macos"))]
        0
    }
}

impl Drop for VoiceAudio {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        unsafe {
            ffi::GhostexVoiceAudioStop(self.handle as *mut std::ffi::c_void)
        };
        // The engine is stopped, but an audio thread may still be finishing a callback: forget the
        // callbacks now and free their context a moment later.
        let context = unsafe { &*self.context };
        if let Ok(mut capture) = context.capture.lock() {
            *capture = None;
        }
        if let Ok(mut device_changed) = context.device_changed.lock() {
            *device_changed = None;
        }
        let address = self.context as usize;
        let _ = std::thread::Builder::new()
            .name("ghostex-voice-audio-release".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                drop(unsafe { Box::from_raw(address as *mut CaptureContext) });
            });
    }
}
