//! Grok dictation for the chat box's and the terminal bar's Dictate: GPUI Kit's speech input with
//! gxserver's `/api/voice?mode=dictation` as its recognizer and this app's audio engine as its
//! microphone.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "the one in the chat box just pastes whatever i say into the chat box, while the one above is more general and for discussing". Dictate turns speech into text with xAI speech-to-text through gxserver, with the xAI key kept on this computer, instead of the system speech recognizer; the words go into the draft (or the terminal) and are never sent (CDXC:SessionChat 2026-10-08 DECISION).

use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use futures::StreamExt as _;
use ghostex_gx_chat_client::voice::{VoiceInbound, VoiceSocket};
use gpui::{App, Context, Subscription};
use gpui_component::speech::{
    AudioFormat, AudioInput, AudioSink, RecognitionSession, SpeechError, SpeechRecognizer,
    SpeechSink, SpeechState,
};
use serde_json::{Value, json};

use super::audio::{AudioStartError, VoiceAudio};
use super::conversation::MAX_AUDIO_RESTARTS;
use super::endpoint::local_voice_endpoint;

/// gxserver's dictation audio: PCM16, mono, 24 kHz.
const DICTATION_FORMAT: (u32, u16) = (24_000, 1);
/// How long Stop waits for gxserver's last words before typing what it has. xAI flushes in about a
/// second.
const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// A speech state that dictates through Grok: Dictate's microphone, Grok's words, and no system
/// recognizer behind it.
pub(crate) fn grok_dictation_speech_state(cx: &mut Context<SpeechState>) -> SpeechState {
    SpeechState::new(cx)
        .recognizer(GrokDictation)
        .input(VoiceMicrophone)
        .system_fallback(false)
        .stop_timeout(STOP_TIMEOUT)
}

/// gxserver's dictation stream as a GPUI Kit recognizer.
struct GrokDictation;

impl SpeechRecognizer for GrokDictation {
    fn audio_format(&self) -> AudioFormat {
        AudioFormat::new(DICTATION_FORMAT.0, DICTATION_FORMAT.1)
    }

    fn start(
        &self,
        sink: SpeechSink,
        cx: &mut App,
    ) -> Result<Box<dyn RecognitionSession>, SpeechError> {
        let endpoint = local_voice_endpoint()
            .ok_or_else(|| SpeechError::recognizer(anyhow!("Ghostex's service is not running")))?;
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<VoiceInbound>();
        let socket = VoiceSocket::open(
            &endpoint,
            &[("mode", "dictation"), ("client", "desktop")],
            Arc::new(move |inbound| {
                let _ = sender.unbounded_send(inbound);
            }),
        );
        cx.spawn(async move |cx| {
            let mut done = false;
            while let Some(inbound) = receiver.next().await {
                let sink = sink.clone();
                let finished = cx.update(|cx| apply(&sink, inbound, &mut done, cx));
                if finished {
                    break;
                }
            }
        })
        .detach();
        Ok(Box::new(GrokDictationSession {
            socket,
            ended: false,
        }))
    }
}

/// Hands one socket event to the speech state. Answers whether the session is over.
fn apply(sink: &SpeechSink, inbound: VoiceInbound, done: &mut bool, cx: &mut App) -> bool {
    match inbound {
        VoiceInbound::Event(event) => {
            let text = || event["text"].as_str().unwrap_or_default().to_string();
            match event["type"].as_str() {
                Some("ready") => sink.ready(cx),
                // `text` is the whole dictation so far, so it replaces the hypothesis.
                Some("transcript") => sink.hypothesis(text(), cx),
                Some("done") => {
                    *done = true;
                    sink.hypothesis(text(), cx);
                    sink.finish(cx);
                    return true;
                }
                Some("error") if event["fatal"].as_bool() != Some(false) => {
                    *done = true;
                    sink.error(SpeechError::recognizer(anyhow!(error_sentence(&event))), cx);
                    return true;
                }
                _ => {}
            }
            false
        }
        VoiceInbound::Audio(_) => false,
        VoiceInbound::Closed(reason) => {
            if !*done {
                match reason {
                    Some(message) => sink.error(SpeechError::recognizer(anyhow!(message)), cx),
                    None => sink.finish(cx),
                }
            }
            true
        }
    }
}

/// gxserver's own sentence for an error frame (it names Settings > Voice agent for a missing key).
fn error_sentence(event: &Value) -> String {
    event["message"]
        .as_str()
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .unwrap_or("Dictation stopped working")
        .to_string()
}

struct GrokDictationSession {
    socket: VoiceSocket,
    ended: bool,
}

impl RecognitionSession for GrokDictationSession {
    fn push_audio(&mut self, samples: &[i16], _cx: &mut App) {
        if self.ended {
            return;
        }
        let bytes = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        self.socket.send_audio(bytes);
    }

    fn finish(&mut self, _cx: &mut App) {
        if !self.ended {
            self.ended = true;
            self.socket.send_json(&json!({ "type": "end" }));
        }
    }
}

impl Drop for GrokDictationSession {
    fn drop(&mut self) {
        // Dropped before Stop means cancelled: gxserver closes without typing anything.
        if !self.ended {
            self.socket.send_json(&json!({ "type": "cancel" }));
        }
    }
}

/// Dictate's microphone: this app's audio engine without echo cancellation or playback, already
/// at 24 kHz mono PCM16.
struct VoiceMicrophone;

impl AudioInput for VoiceMicrophone {
    fn start(
        &self,
        _format: AudioFormat,
        sink: AudioSink,
        cx: &mut App,
    ) -> Result<Subscription, SpeechError> {
        let (sender, mut receiver) =
            futures::channel::mpsc::unbounded::<Result<Vec<i16>, AudioStartError>>();
        let running: Arc<Mutex<Option<VoiceAudio>>> = Arc::new(Mutex::new(None));
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let running = running.clone();
            let stopped = stopped.clone();
            let _ = std::thread::Builder::new()
                .name("ghostex-dictation-microphone".into())
                .spawn(move || {
                    // Bluetooth earbuds switch to their headset profile once the microphone opens,
                    // which stops the engine; it is started again, as a conversation does.
                    let (restart, restart_requested) = std::sync::mpsc::channel::<()>();
                    for _ in 0..=MAX_AUDIO_RESTARTS {
                        let capture_sender = sender.clone();
                        let restart = restart.clone();
                        let started = VoiceAudio::start(
                            false,
                            false,
                            Box::new(move |samples| {
                                let _ = capture_sender.unbounded_send(Ok(samples.to_vec()));
                            }),
                            Box::new(move || {
                                let _ = restart.send(());
                            }),
                        );
                        match started {
                            // Checked under the lock the stop takes, so a stop that came first
                            // always finds the audio or has it dropped here.
                            Ok(audio) => {
                                let Ok(mut slot) = running.lock() else {
                                    return;
                                };
                                if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                                    return;
                                }
                                *slot = Some(audio);
                            }
                            Err(error) => {
                                let _ = sender.unbounded_send(Err(error));
                                return;
                            }
                        }
                        loop {
                            match restart_requested
                                .recv_timeout(std::time::Duration::from_millis(200))
                            {
                                Ok(()) => break,
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                                    if !stopped.load(std::sync::atomic::Ordering::SeqCst) => {}
                                Err(_) => return,
                            }
                        }
                        if let Ok(mut slot) = running.lock() {
                            slot.take();
                        }
                        if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                            return;
                        }
                    }
                });
        }
        let task = cx.spawn(async move |cx| {
            while let Some(captured) = receiver.next().await {
                let sink = sink.clone();
                let ended = cx.update(|cx| match captured {
                    Ok(samples) => {
                        sink.push(samples, cx);
                        false
                    }
                    Err(error) => {
                        sink.error(
                            match error {
                                AudioStartError::PermissionDenied => SpeechError::PermissionDenied,
                                AudioStartError::NoInput => SpeechError::NoInputDevice,
                                AudioStartError::Unsupported => SpeechError::Unsupported,
                                AudioStartError::Failed(message) => {
                                    SpeechError::input(anyhow!(message))
                                }
                            },
                            cx,
                        );
                        true
                    }
                });
                if ended {
                    break;
                }
            }
        });
        Ok(Subscription::new(move || {
            stopped.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Ok(mut slot) = running.lock() {
                slot.take();
            }
            drop(task);
        }))
    }
}
